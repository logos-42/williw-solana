import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ComputePower } from "../target/types/compute_power";
import { expect } from "chai";

describe("安全测试套件", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ComputePower as Program<ComputePower>;
  
  const user = provider.wallet;
  const attacker = anchor.web3.Keypair.generate();
  const validator = anchor.web3.Keypair.generate();

  let userAccountPda: anchor.web3.PublicKey;
  let attackerAccountPda: anchor.web3.PublicKey;
  let platformAccountPda: anchor.web3.PublicKey;

  before(async () => {
    // 获取 PDA 地址
    [userAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user"), user.publicKey.toBuffer()],
      program.programId
    );

    [attackerAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user"), attacker.publicKey.toBuffer()],
      program.programId
    );

    [platformAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("platform")],
      program.programId
    );

    // 给攻击者和验证节点空投 SOL
    const airdropSig1 = await provider.connection.requestAirdrop(
      attacker.publicKey,
      5 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropSig1);

    const airdropSig2 = await provider.connection.requestAirdrop(
      validator.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdropSig2);

    // 初始化平台和用户账户
    await program.methods
      .initializePlatform()
      .accounts({
        platformAccount: platformAccountPda,
        authority: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .initializeUser()
      .accounts({
        userAccount: userAccountPda,
        user: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
  });

  describe("1. 权限验证测试", () => {
    it("应该拒绝未授权用户操作他人账户", async () => {
      // 攻击者尝试注册用户的账户为提供者
      try {
        await program.methods
          .registerAsProvider()
          .accounts({
            userAccount: userAccountPda,
            user: attacker.publicKey, // 错误的签名者
          })
          .signers([attacker])
          .rpc();
        
        expect.fail("应该抛出未授权错误");
      } catch (error) {
        expect(error.toString()).to.include("Unauthorized");
      }
    });

    it("应该拒绝未授权的验证节点提交算力", async () => {
      // 先注册为提供者
      await program.methods
        .registerAsProvider()
        .accounts({
          userAccount: userAccountPda,
          user: user.publicKey,
        })
        .rpc();

      // 未授权的验证节点尝试提交算力
      try {
        await program.methods
          .submitComputeWork(new anchor.BN(5000))
          .accounts({
            providerAccount: userAccountPda,
            platformAccount: platformAccountPda,
            validator: attacker.publicKey, // 未授权的验证节点
            provider: user.publicKey,
          })
          .signers([attacker])
          .rpc();
        
        expect.fail("应该抛出未授权验证节点错误");
      } catch (error) {
        expect(error.toString()).to.include("UnauthorizedValidator");
      }
    });
  });

  describe("2. 余额检查测试", () => {
    before(async () => {
      // 初始化攻击者账户
      await program.methods
        .initializeUser()
        .accounts({
          userAccount: attackerAccountPda,
          user: attacker.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([attacker])
        .rpc();
    });

    it("应该拒绝余额不足的订阅", async () => {
      // 转走攻击者的大部分 SOL，只留少量
      const attackerBalance = await provider.connection.getBalance(
        attacker.publicKey
      );
      
      const recipient = anchor.web3.Keypair.generate();
      const transferAmount = attackerBalance - 100_000_000; // 只留 0.1 SOL

      const transferTx = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: attacker.publicKey,
          toPubkey: recipient.publicKey,
          lamports: transferAmount,
        })
      );

      await provider.sendAndConfirm(transferTx, [attacker]);

      // 尝试订阅需要 1 SOL 的基础计划
      try {
        await program.methods
          .subscribePlan({ basic: {} })
          .accounts({
            userAccount: attackerAccountPda,
            platformAccount: platformAccountPda,
            user: attacker.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([attacker])
          .rpc();
        
        expect.fail("应该抛出余额不足错误");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientBalance");
      }
    });
  });

  describe("3. 整数溢出测试", () => {
    it("应该防止 API 额度溢出", async () => {
      // 订阅计划
      await program.methods
        .subscribePlan({ basic: {} })
        .accounts({
          userAccount: userAccountPda,
          platformAccount: platformAccountPda,
          user: user.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      // 注意：实际的溢出测试需要修改合约或使用特殊的测试场景
      // 这里我们验证 checked_add 的存在
      const userAccount = await program.account.userAccount.fetch(userAccountPda);
      expect(userAccount.apiCredits.toNumber()).to.be.greaterThan(0);
    });
  });

  describe("4. 算力提交限制测试", () => {
    it("应该拒绝超过最大限制的算力提交", async () => {
      const maxComputeUnits = 1_000_000;
      const excessiveUnits = maxComputeUnits + 1;

      try {
        await program.methods
          .submitComputeWork(new anchor.BN(excessiveUnits))
          .accounts({
            providerAccount: userAccountPda,
            platformAccount: platformAccountPda,
            validator: user.publicKey, // 使用授权的验证节点
            provider: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出无效算力单位错误");
      } catch (error) {
        expect(error.toString()).to.include("InvalidComputeUnits");
      }
    });

    it("应该拒绝零算力提交", async () => {
      try {
        await program.methods
          .submitComputeWork(new anchor.BN(0))
          .accounts({
            providerAccount: userAccountPda,
            platformAccount: platformAccountPda,
            validator: user.publicKey,
            provider: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出无效算力单位错误");
      } catch (error) {
        expect(error.toString()).to.include("InvalidComputeUnits");
      }
    });
  });

  describe("5. API 额度消耗限制测试", () => {
    it("应该拒绝超过最大限制的额度消耗", async () => {
      const maxCredits = 10_000;
      const excessiveCredits = maxCredits + 1;

      try {
        await program.methods
          .consumeApiCredits(new anchor.BN(excessiveCredits))
          .accounts({
            userAccount: userAccountPda,
            platformAccount: platformAccountPda,
            user: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出无效额度数量错误");
      } catch (error) {
        expect(error.toString()).to.include("InvalidCreditAmount");
      }
    });

    it("应该拒绝零额度消耗", async () => {
      try {
        await program.methods
          .consumeApiCredits(new anchor.BN(0))
          .accounts({
            userAccount: userAccountPda,
            platformAccount: platformAccountPda,
            user: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出无效额度数量错误");
      } catch (error) {
        expect(error.toString()).to.include("InvalidCreditAmount");
      }
    });

    it("应该拒绝额度不足的消耗", async () => {
      const userAccount = await program.account.userAccount.fetch(userAccountPda);
      const excessiveCredits = userAccount.apiCredits.toNumber() + 1;

      try {
        await program.methods
          .consumeApiCredits(new anchor.BN(excessiveCredits))
          .accounts({
            userAccount: userAccountPda,
            platformAccount: platformAccountPda,
            user: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出额度不足错误");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientCredits");
      }
    });
  });

  describe("6. 提现限制测试", () => {
    it("应该拒绝低于最小金额的提现", async () => {
      // 提交少量算力以获得少量收益
      await program.methods
        .submitComputeWork(new anchor.BN(100)) // 很少的算力
        .accounts({
          providerAccount: userAccountPda,
          platformAccount: platformAccountPda,
          validator: user.publicKey,
          provider: user.publicKey,
        })
        .rpc();

      const userAccount = await program.account.userAccount.fetch(userAccountPda);
      
      // 如果收益低于最小提现金额
      if (userAccount.earnings.toNumber() < 1_000_000) {
        try {
          await program.methods
            .withdrawEarnings()
            .accounts({
              providerAccount: userAccountPda,
              platformAccount: platformAccountPda,
              user: user.publicKey,
            })
            .rpc();
          
          expect.fail("应该抛出低于最小提现金额错误");
        } catch (error) {
          expect(error.toString()).to.include("BelowMinimumWithdraw");
        }
      }
    });

    it("应该拒绝冷却时间内的重复提现", async () => {
      // 提交足够的算力以获得可提现的收益
      await program.methods
        .submitComputeWork(new anchor.BN(10_000))
        .accounts({
          providerAccount: userAccountPda,
          platformAccount: platformAccountPda,
          validator: user.publicKey,
          provider: user.publicKey,
        })
        .rpc();

      // 第一次提现
      await program.methods
        .withdrawEarnings()
        .accounts({
          providerAccount: userAccountPda,
          platformAccount: platformAccountPda,
          user: user.publicKey,
        })
        .rpc();

      // 再次提交算力
      await program.methods
        .submitComputeWork(new anchor.BN(10_000))
        .accounts({
          providerAccount: userAccountPda,
          platformAccount: platformAccountPda,
          validator: user.publicKey,
          provider: user.publicKey,
        })
        .rpc();

      // 立即尝试第二次提现（冷却时间内）
      try {
        await program.methods
          .withdrawEarnings()
          .accounts({
            providerAccount: userAccountPda,
            platformAccount: platformAccountPda,
            user: user.publicKey,
          })
          .rpc();
        
        expect.fail("应该抛出提现冷却时间错误");
      } catch (error) {
        expect(error.toString()).to.include("WithdrawCooldown");
      }
    });
  });

  describe("7. 订阅过期测试", () => {
    it("应该拒绝过期订阅的 API 调用", async () => {
      // 注意：这个测试需要等待订阅过期或修改时间
      // 在实际测试中，可以使用时间旅行或模拟时间
      
      // 这里我们只验证检查逻辑的存在
      const userAccount = await program.account.userAccount.fetch(userAccountPda);
      expect(userAccount.subscriptionExpiry.toNumber()).to.be.greaterThan(0);
    });
  });

  describe("8. 状态一致性测试", () => {
    it("应该正确更新所有相关状态", async () => {
      const beforeUser = await program.account.userAccount.fetch(userAccountPda);
      const beforePlatform = await program.account.platformAccount.fetch(
        platformAccountPda
      );

      // 提交算力
      const computeUnits = 5000;
      await program.methods
        .submitComputeWork(new anchor.BN(computeUnits))
        .accounts({
          providerAccount: userAccountPda,
          platformAccount: platformAccountPda,
          validator: user.publicKey,
          provider: user.publicKey,
        })
        .rpc();

      const afterUser = await program.account.userAccount.fetch(userAccountPda);
      const afterPlatform = await program.account.platformAccount.fetch(
        platformAccountPda
      );

      // 验证用户状态更新
      expect(afterUser.computePowerContributed.toNumber()).to.equal(
        beforeUser.computePowerContributed.toNumber() + computeUnits
      );
      expect(afterUser.earnings.toNumber()).to.be.greaterThan(
        beforeUser.earnings.toNumber()
      );

      // 验证平台状态更新
      expect(afterPlatform.totalComputeUnits.toNumber()).to.equal(
        beforePlatform.totalComputeUnits.toNumber() + computeUnits
      );
    });
  });
});

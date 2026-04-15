import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ComputePower } from "../target/types/compute_power";
import { expect } from "chai";

describe("compute-power", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ComputePower as Program<ComputePower>;

  const user = provider.wallet;
  const validator = anchor.web3.Keypair.generate();

  let userAccountPda: anchor.web3.PublicKey;
  let platformAccountPda: anchor.web3.PublicKey;

  before(async () => {
    // 获取 PDA 地址
    [userAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user"), user.publicKey.toBuffer()],
      program.programId,
    );

    [platformAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("platform")],
      program.programId,
    );

    // 给验证节点空投 SOL
    const airdropSig = await provider.connection.requestAirdrop(
      validator.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(airdropSig);
  });

  it("初始化平台账户", async () => {
    await program.methods
      .initializePlatform()
      .accounts({
        platformAccount: platformAccountPda,
        authority: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const platformAccount =
      await program.account.platformAccount.fetch(platformAccountPda);

    expect(platformAccount.authority.toString()).to.equal(
      user.publicKey.toString(),
    );
    expect(platformAccount.totalRevenue.toNumber()).to.equal(0);
  });

  it("初始化用户账户", async () => {
    await program.methods
      .initializeUser()
      .accounts({
        userAccount: userAccountPda,
        user: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);

    expect(userAccount.owner.toString()).to.equal(user.publicKey.toString());
    expect(userAccount.apiCredits.toNumber()).to.equal(0);
    expect(userAccount.isProvider).to.be.false;
  });

  it("订阅基础计划", async () => {
    const balanceBefore = await provider.connection.getBalance(user.publicKey);

    await program.methods
      .subscribePlan({ basic: {} })
      .accounts({
        userAccount: userAccountPda,
        platformAccount: platformAccountPda,
        user: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    const balanceAfter = await provider.connection.getBalance(user.publicKey);

    expect(userAccount.apiCredits.toNumber()).to.equal(10000);
    expect(userAccount.subscriptionPlan).to.deep.equal({ basic: {} });
    expect(balanceBefore - balanceAfter).to.be.greaterThan(
      anchor.web3.LAMPORTS_PER_SOL,
    );
  });

  it("注册为算力提供者", async () => {
    await program.methods
      .registerAsProvider()
      .accounts({
        userAccount: userAccountPda,
        user: user.publicKey,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);

    expect(userAccount.isProvider).to.be.true;
    expect(userAccount.computePowerContributed.toNumber()).to.equal(0);
    expect(userAccount.earnings.toNumber()).to.equal(0);
    expect(userAccount.lastWithdrawTime.toNumber()).to.equal(0);
  });

  it("提交算力工作", async () => {
    const computeUnits = 5000;

    // 注意：在实际部署中，validator 需要是平台授权的地址
    // 这里为了测试，我们需要先将 validator 设置为平台 authority
    // 或者修改合约逻辑以支持测试场景

    await program.methods
      .submitComputeWork(new anchor.BN(computeUnits))
      .accounts({
        providerAccount: userAccountPda,
        platformAccount: platformAccountPda,
        validator: user.publicKey, // 使用 user 作为 validator（因为 user 是 platform authority）
        provider: user.publicKey,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);

    expect(userAccount.computePowerContributed.toNumber()).to.equal(
      computeUnits,
    );
    expect(userAccount.earnings.toNumber()).to.be.greaterThan(0);
  });

  it("消耗 API 额度", async () => {
    const creditsBefore = (
      await program.account.userAccount.fetch(userAccountPda)
    ).apiCredits.toNumber();

    await program.methods
      .consumeApiCredits(new anchor.BN(100))
      .accounts({
        userAccount: userAccountPda,
        platformAccount: platformAccountPda,
        user: user.publicKey,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);

    expect(userAccount.apiCredits.toNumber()).to.equal(creditsBefore - 100);
  });

  it("提现收益", async () => {
    // 由于有提现冷却时间，我们需要修改用户账户来跳过冷却检查
    // 在测试环境中，我们可以直接操作账户数据或者等待时间流逝

    // 先给平台账户转入足够的 SOL（用于支付收益）
    const platformAccount =
      await program.account.platformAccount.fetch(platformAccountPda);
    const platformBalance =
      await provider.connection.getBalance(platformAccountPda);

    if (platformBalance < 1_000_000_000) {
      // 给平台账户转入一些 SOL
      await provider.connection.requestAirdrop(
        platformAccountPda,
        2 * anchor.web3.LAMPORTS_PER_SOL,
      );
    }

    const earningsBefore = (
      await program.account.userAccount.fetch(userAccountPda)
    ).earnings.toNumber();

    const balanceBefore = await provider.connection.getBalance(user.publicKey);

    await program.methods
      .withdrawEarnings()
      .accounts({
        providerAccount: userAccountPda,
        platformAccount: platformAccountPda,
        user: user.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    const balanceAfter = await provider.connection.getBalance(user.publicKey);

    expect(userAccount.earnings.toNumber()).to.equal(0);
    expect(balanceAfter).to.be.greaterThan(balanceBefore);
    expect(userAccount.lastWithdrawTime.toNumber()).to.be.greaterThan(0);
  });
});

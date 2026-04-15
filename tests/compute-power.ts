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
      program.programId
    );

    [platformAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("platform")],
      program.programId
    );

    // 给验证节点空投 SOL
    const airdropSig = await provider.connection.requestAirdrop(
      validator.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
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

    const platformAccount = await program.account.platformAccount.fetch(
      platformAccountPda
    );

    expect(platformAccount.authority.toString()).to.equal(
      user.publicKey.toString()
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
      anchor.web3.LAMPORTS_PER_SOL
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
  });

  it("提交算力工作", async () => {
    const computeUnits = 5000;

    await program.methods
      .submitComputeWork(new anchor.BN(computeUnits))
      .accounts({
        providerAccount: userAccountPda,
        platformAccount: platformAccountPda,
        validator: validator.publicKey,
        provider: user.publicKey,
      })
      .signers([validator])
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);

    expect(userAccount.computePowerContributed.toNumber()).to.equal(
      computeUnits
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
      })
      .rpc();

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    const balanceAfter = await provider.connection.getBalance(user.publicKey);

    expect(userAccount.earnings.toNumber()).to.equal(0);
    expect(balanceAfter).to.be.greaterThan(balanceBefore);
  });
});

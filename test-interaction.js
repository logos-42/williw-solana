import * as anchor from "@coral-xyz/anchor";
import { Connection, PublicKey } from "@solana/web3.js";

const PROGRAM_ID = "7ng11Nyy312EbcsKmEnugTfBnkCLpiqxVLMCwBSUVK3a";
const RPC_URL = "https://api.devnet.solana.com";

async function main() {
  const connection = new Connection(RPC_URL, "confirmed");
  const wallet = anchor.Wallet.local();

  const program = new anchor.Program(
    {
      version: "1.0.0",
      name: "compute_power",
      instructions: [],
      accounts: [],
      metadata: {
        name: "compute_power",
        version: "1.0.0",
        spec: "1.0.0",
        identifier: PROGRAM_ID,
        authority: wallet.publicKey.toString(),
        compat: { major: 1, minor: 0 },
      },
    },
    new PublicKey(PROGRAM_ID),
    new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" }),
  );

  const [userAccountPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("user"), wallet.publicKey.toBuffer()],
    program.programId,
  );

  const [platformAccountPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("platform")],
    program.programId,
  );

  console.log("Wallet:", wallet.publicKey.toString());
  console.log("User PDA:", userAccountPda.toString());
  console.log("Platform PDA:", platformAccountPda.toString());

  try {
    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    console.log("\nUser Account exists:");
    console.log("  - Owner:", userAccount.owner.toString());
    console.log("  - isProvider:", userAccount.isProvider);
    console.log("  - API Credits:", userAccount.apiCredits.toString());
    console.log("  - Earnings:", userAccount.earnings.toString());
  } catch (e) {
    console.log("\nUser Account does not exist yet");
  }

  try {
    const platformAccount =
      await program.account.platformAccount.fetch(platformAccountPda);
    console.log("\nPlatform Account exists:");
    console.log("  - Authority:", platformAccount.authority.toString());
    console.log("  - Total Revenue:", platformAccount.totalRevenue.toString());
    console.log(
      "  - Total API Calls:",
      platformAccount.totalApiCalls.toString(),
    );
  } catch (e) {
    console.log("\nPlatform Account does not exist yet");
  }
}

main().catch(console.error);

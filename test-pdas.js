import { Connection, PublicKey } from "@solana/web3.js";
import { Keypair } from "@solana/web3.js";
import * as fs from "fs";

// Config
const RPC_URL = "https://api.devnet.solana.com";
const PROGRAM_ID = "7ng11Nyy312EbcsKmEnugTfBnkCLpiqxVLMCwBSUVK3a";

// Load wallet
const keypairData = JSON.parse(
  fs.readFileSync("/Users/apple/.config/solana/id.json", "utf-8"),
);
const wallet = Keypair.fromSecretKey(new Uint8Array(keypairData));

// PDAs
const userPda = PublicKey.findProgramAddressSync(
  [Buffer.from("user"), wallet.publicKey.toBuffer()],
  new PublicKey(PROGRAM_ID),
)[0];

const platformPda = PublicKey.findProgramAddressSync(
  [Buffer.from("platform")],
  new PublicKey(PROGRAM_ID),
)[0];

console.log("Wallet:", wallet.publicKey.toString());
console.log("User PDA:", userPda.toString());
console.log("Platform PDA:", platformPda.toString());
console.log("Program ID:", PROGRAM_ID);

// Build InitializeUser instruction (simplified - just send transaction)
const connection = new Connection(RPC_URL, "confirmed");

async function test() {
  // Check if accounts exist
  try {
    const userInfo = await connection.getAccountInfo(userPda);
    console.log("\nUser Account exists:", !!userInfo);
    if (userInfo) console.log("  Data length:", userInfo.data.length);
  } catch (e) {
    console.log("\nUser Account error:", e.message);
  }

  try {
    const platformInfo = await connection.getAccountInfo(platformPda);
    console.log("Platform Account exists:", !!platformInfo);
    if (platformInfo) console.log("  Data length:", platformInfo.data.length);
  } catch (e) {
    console.log("Platform Account error:", e.message);
  }
}

test().catch(console.error);

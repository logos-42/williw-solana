import {
  Connection,
  PublicKey,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  Keypair,
} from "@solana/web3.js";
import * as fs from "fs";

// Config
const RPC_URL = "https://api.devnet.solana.com";
const PROGRAM_ID = "9773x8BctiQXjgJeEmou9FXFADgidKVKUH71zpcngv1f";

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

const connection = new Connection(RPC_URL, "confirmed");

async function initializePlatform() {
  // Create initialize_platform instruction
  // Using raw transaction with createAccount for PDA

  const instruction = SystemProgram.createAccount({
    fromPubkey: wallet.publicKey,
    newAccountPubkey: platformPda,
    lamports: await connection.getMinimumBalanceForRentExemption(200), // Approximate size
    space: 200,
    programId: new PublicKey(PROGRAM_ID),
  });

  // Actually we need to call the program, not create account
  // Let me create a proper ix using a simplified approach

  const ixData = Buffer.from([0x00]); // Method index for initialize_platform (check lib.rs)

  const ix = new TransactionInstruction({
    keys: [
      { pubkey: platformPda, isWritable: true, isSigner: false },
      { pubkey: wallet.publicKey, isWritable: true, isSigner: true },
      { pubkey: SystemProgram.programId, isWritable: false, isSigner: false },
    ],
    programId: new PublicKey(PROGRAM_ID),
    data: ixData,
  });

  const tx = new Transaction().add(ix);
  tx.feePayer = wallet.publicKey;

  const { blockhash } = await connection.getLatestBlockhash();
  tx.recentBlockhash = blockhash;

  tx.sign(wallet);

  const sig = await connection.sendTransaction(tx, [wallet]);
  console.log("Initialize Platform tx:", sig);

  await connection.confirmTransaction(sig);
  console.log("Platform initialized!");
}

async function initializeUser() {
  const ixData = Buffer.from([0x01]); // Method index for initialize_user

  const ix = new TransactionInstruction({
    keys: [
      { pubkey: userPda, isWritable: true, isSigner: false },
      { pubkey: wallet.publicKey, isWritable: true, isSigner: true },
      { pubkey: SystemProgram.programId, isWritable: false, isSigner: false },
    ],
    programId: new PublicKey(PROGRAM_ID),
    data: ixData,
  });

  const tx = new Transaction().add(ix);
  tx.feePayer = wallet.publicKey;

  const { blockhash } = await connection.getLatestBlockhash();
  tx.recentBlockhash = blockhash;

  tx.sign(wallet);

  const sig = await connection.sendTransaction(tx, [wallet]);
  console.log("Initialize User tx:", sig);

  await connection.confirmTransaction(sig);
  console.log("User initialized!");
}

async function test() {
  // Check if accounts exist
  const userInfo = await connection.getAccountInfo(userPda);
  const platformInfo = await connection.getAccountInfo(platformPda);

  console.log("\nBefore initialization:");
  console.log("  User Account exists:", !!userInfo);
  console.log("  Platform Account exists:", !!platformInfo);

  if (!platformInfo) {
    console.log("\nInitializing Platform...");
    try {
      await initializePlatform();
    } catch (e) {
      console.log("Platform init error:", e.message);
    }
  }

  if (!userInfo) {
    console.log("\nInitializing User...");
    try {
      await initializeUser();
    } catch (e) {
      console.log("User init error:", e.message);
    }
  }

  // Check again
  const userInfoAfter = await connection.getAccountInfo(userPda);
  const platformInfoAfter = await connection.getAccountInfo(platformPda);

  console.log("\nAfter initialization:");
  console.log("  User Account exists:", !!userInfoAfter);
  console.log("  Platform Account exists:", !!platformInfoAfter);
}

test().catch(console.error);

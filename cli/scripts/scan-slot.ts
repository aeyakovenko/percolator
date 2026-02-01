import { Connection, PublicKey } from "@solana/web3.js";
import * as fs from "fs";

const connection = new Connection("https://api.devnet.solana.com", "confirmed");
const config = JSON.parse(fs.readFileSync("../devnet-config.json", "utf-8"));
const slab = new PublicKey(config.slab);

async function main() {
  const info = await connection.getAccountInfo(slab);
  if (!info) { console.log("Not found"); return; }
  
  const currentSlot = await connection.getSlot();
  console.log("Current slot:", currentSlot);
  
  // Recent crank slot value to search for
  const targetSlot = BigInt(435269014);
  const targetBytes = Buffer.alloc(8);
  targetBytes.writeBigUInt64LE(targetSlot);
  const targetHex = targetBytes.toString('hex');
  
  console.log("Searching for slot value:", targetSlot, "(hex:", targetHex, ")");
  
  // Scan the buffer for this value
  for (let off = 0; off < info.data.length - 7; off++) {
    const val = info.data.readBigUInt64LE(off);
    if (val === targetSlot) {
      console.log("Found at offset", off, "(engine offset", off - 328, ")");
    }
  }
  
  // Also look for any slot-like values (400M-440M range)
  console.log("\n--- Scanning for slot-like values (400M-440M range) ---");
  const found = new Map<number, bigint>();
  for (let off = 0; off < info.data.length - 7; off++) {
    const val = info.data.readBigUInt64LE(off);
    if (val >= 400_000_000n && val <= 440_000_000n) {
      found.set(off, val);
    }
  }
  
  // Group by value and show
  const byValue = new Map<string, number[]>();
  found.forEach((val, off) => {
    const key = val.toString();
    if (!byValue.has(key)) byValue.set(key, []);
    byValue.get(key)!.push(off);
  });
  
  byValue.forEach((offsets, val) => {
    console.log(`Value ${val} found at offsets:`, offsets.slice(0, 10).join(', '), offsets.length > 10 ? `... (${offsets.length} total)` : '');
  });
}

main().catch(console.error);

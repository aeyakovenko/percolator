import { Connection, PublicKey } from "@solana/web3.js";
import * as fs from "fs";

const connection = new Connection("https://api.devnet.solana.com", "confirmed");
const config = JSON.parse(fs.readFileSync("../devnet-config.json", "utf-8"));
const slab = new PublicKey(config.slab);

const ENGINE_OFF = 328;
const BITMAP_OFF = 86520;

async function main() {
  const info = await connection.getAccountInfo(slab);
  if (!info) { console.log("Not found"); return; }
  
  console.log("Slab size:", info.data.length);
  console.log("Engine starts at:", ENGINE_OFF);
  console.log("Bitmap starts at engine offset:", BITMAP_OFF, "(slab offset", ENGINE_OFF + BITMAP_OFF, ")");
  
  // Read the 200 bytes before bitmap to understand the layout
  console.log("\n--- Raw bytes before bitmap (engine offset 86300-86520) ---");
  for (let off = 86300; off < 86520; off += 8) {
    const slabOff = ENGINE_OFF + off;
    const bytes = info.data.subarray(slabOff, slabOff + 8);
    const hex = Buffer.from(bytes).toString("hex");
    const u64 = info.data.readBigUInt64LE(slabOff);
    const u8 = info.data.readUInt8(slabOff);
    const u16 = info.data.readUInt16LE(slabOff);
    
    // Highlight non-zero values
    if (u64 !== 0n) {
      console.log(`@${off}: ${hex} (u64=${u64}, u16=${u16}, u8=${u8}) ***`);
    } else {
      console.log(`@${off}: ${hex} (u64=${u64})`);
    }
  }
  
  // Also check at the offsets found in slot scan
  console.log("\n--- Check offsets 86416, 86424 (engine) ---");
  const off1 = 86416;
  const off2 = 86424;
  console.log(`@${off1}:`, info.data.readBigUInt64LE(ENGINE_OFF + off1).toString());
  console.log(`@${off2}:`, info.data.readBigUInt64LE(ENGINE_OFF + off2).toString());
  
  // Check if sweep fields might be at offset 360 after all (maybe in a simpler struct layout)
  console.log("\n--- Check at old offsets 360, 368, 376 ---");
  console.log("@360:", info.data.readBigUInt64LE(ENGINE_OFF + 360).toString());
  console.log("@368:", info.data.readBigUInt64LE(ENGINE_OFF + 368).toString());
  console.log("@376 (u8):", info.data.readUInt8(ENGINE_OFF + 376));
}

main().catch(console.error);

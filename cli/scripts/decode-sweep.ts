import { Connection, PublicKey } from "@solana/web3.js";
import * as fs from "fs";

const connection = new Connection("https://api.devnet.solana.com", "confirmed");
const config = JSON.parse(fs.readFileSync("../devnet-config.json", "utf-8"));
const slab = new PublicKey(config.slab);

const ENGINE_OFF = 328;

async function main() {
  const info = await connection.getAccountInfo(slab);
  if (!info) { console.log("Not found"); return; }
  
  const currentSlot = await connection.getSlot();
  console.log("Current slot:", currentSlot);
  
  // Test reading at engine offset 86412
  // Layout should be:
  // @86412: liq_cursor (u16) - 2 bytes
  // @86414: gc_cursor (u16) - 2 bytes
  // @86416: last_full_sweep_start_slot (u64) - 8 bytes
  // @86424: last_full_sweep_completed_slot (u64) - 8 bytes
  // @86432: crank_step (u8) - 1 byte
  
  const base = ENGINE_OFF;
  const liq_cursor = info.data.readUInt16LE(base + 86412);
  const gc_cursor = info.data.readUInt16LE(base + 86414);
  const sweep_start = info.data.readBigUInt64LE(base + 86416);
  const sweep_complete = info.data.readBigUInt64LE(base + 86424);
  const crank_step = info.data.readUInt8(base + 86432);
  
  console.log("\n--- Sweep-related fields at engine offset 86412 ---");
  console.log("liq_cursor @86412:", liq_cursor);
  console.log("gc_cursor @86414:", gc_cursor);
  console.log("last_full_sweep_start_slot @86416:", sweep_start.toString());
  console.log("last_full_sweep_completed_slot @86424:", sweep_complete.toString());
  console.log("crank_step @86432:", crank_step);
  
  // Check staleness
  const maxStaleness = info.data.readBigUInt64LE(base + 288);
  console.log("\nmax_crank_staleness_slots:", maxStaleness.toString());
  console.log("Sweep age:", currentSlot - Number(sweep_start), "slots");
  console.log("Sweep is fresh:", currentSlot - Number(sweep_start) <= Number(maxStaleness));
}

main().catch(console.error);

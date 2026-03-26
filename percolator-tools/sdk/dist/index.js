"use strict";
/**
 * Percolator SDK — types and instruction builders for wrapper programs.
 * Engine does not move tokens; wrapper must do SPL transfers and call engine.
 *
 * @see https://github.com/aeyakovenko/percolator
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.formatBigint = exports.decodeRiskEngine = exports.decodeEngineState = exports.decodeAccountSlab = exports.INSTRUCTION_NAMES = void 0;
exports.buildDepositInstructionData = buildDepositInstructionData;
exports.buildWithdrawInstructionData = buildWithdrawInstructionData;
exports.buildExecuteTradeInstructionData = buildExecuteTradeInstructionData;
exports.buildKeeperCrankInstructionData = buildKeeperCrankInstructionData;
// -----------------------------------------------------------------------------
// Instruction discriminators (wrapper-specific; replace with your program's)
// -----------------------------------------------------------------------------
exports.INSTRUCTION_NAMES = {
    deposit: 0x00,
    withdraw: 0x01,
    executeTrade: 0x02,
    keeperCrank: 0x03,
    addUser: 0x04,
    addLp: 0x05,
};
// -----------------------------------------------------------------------------
// Instruction builders (serialize args for CPI; layout is wrapper-defined)
// -----------------------------------------------------------------------------
function encodeU64(n) {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(BigInt(n));
    return b;
}
function encodeU128(n) {
    const b = Buffer.alloc(16);
    b.writeBigUInt64LE(n & BigInt('0xffffffffffffffff'), 0);
    b.writeBigUInt64LE(n >> BigInt(64), 8);
    return b;
}
function encodeI128(n) {
    const b = Buffer.alloc(16);
    const lo = n & BigInt('0xffffffffffffffff');
    const hi = n >> BigInt(64);
    b.writeBigUInt64LE(lo >= 0 ? lo : lo + BigInt(1) << BigInt(64), 0);
    b.writeBigInt64LE(hi, 8);
    return b;
}
/**
 * Build deposit instruction data.
 * Wrapper must: transfer tokens to vault, then call engine.deposit(idx, amount, now_slot).
 */
function buildDepositInstructionData(args) {
    const parts = [
        Buffer.from([exports.INSTRUCTION_NAMES.deposit]),
        encodeU64(args.accountIndex),
        encodeU128(args.amount),
        encodeU64(args.nowSlot),
    ];
    return Buffer.concat(parts);
}
/**
 * Build withdraw instruction data.
 * Wrapper must: call engine.withdraw(...); if Ok, transfer tokens out of vault.
 */
function buildWithdrawInstructionData(args) {
    const parts = [
        Buffer.from([exports.INSTRUCTION_NAMES.withdraw]),
        encodeU64(args.accountIndex),
        encodeU128(args.amount),
        encodeU64(args.nowSlot),
        encodeU64(args.oraclePrice),
    ];
    return Buffer.concat(parts);
}
/**
 * Build execute_trade instruction data.
 * Wrapper must validate signers and oracle, then call engine.execute_trade(...).
 */
function buildExecuteTradeInstructionData(args) {
    const size = BigInt(args.size);
    const parts = [
        Buffer.from([exports.INSTRUCTION_NAMES.executeTrade]),
        encodeU64(args.lpIndex),
        encodeU64(args.userIndex),
        encodeU64(args.nowSlot),
        encodeU64(args.oraclePrice),
        encodeI128(size),
    ];
    return Buffer.concat(parts);
}
/**
 * Build keeper_crank instruction data.
 * Permissionless; anyone can call. Wrapper forwards to engine.keeper_crank(...).
 */
function buildKeeperCrankInstructionData(args) {
    const parts = [
        Buffer.from([exports.INSTRUCTION_NAMES.keeperCrank]),
        encodeU64(args.callerIndex),
        encodeU64(args.nowSlot),
        encodeU64(args.oraclePrice),
        encodeI128(BigInt(args.fundingRateBpsPerSlot)),
        Buffer.from([args.allowPanic ? 1 : 0]),
    ];
    return Buffer.concat(parts);
}
// -----------------------------------------------------------------------------
// Engine state decoder (read vault, OI, funding, etc. from state account)
// -----------------------------------------------------------------------------
var decodeEngineState_1 = require("./decodeEngineState");
Object.defineProperty(exports, "decodeAccountSlab", { enumerable: true, get: function () { return decodeEngineState_1.decodeAccountSlab; } });
Object.defineProperty(exports, "decodeEngineState", { enumerable: true, get: function () { return decodeEngineState_1.decodeEngineState; } });
Object.defineProperty(exports, "decodeRiskEngine", { enumerable: true, get: function () { return decodeEngineState_1.decodeRiskEngine; } });
Object.defineProperty(exports, "formatBigint", { enumerable: true, get: function () { return decodeEngineState_1.formatBigint; } });

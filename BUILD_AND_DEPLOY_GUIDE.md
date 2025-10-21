# 🔨 Build & Deploy Percolator Programs - Complete Guide

**Objective**: Build Router and Slab programs with `cargo build-sbf` and deploy to local Solana validator  
**Time**: ~15-20 minutes  
**Difficulty**: Medium (requires admin/elevated privileges)

---

## ⚠️ Critical Prerequisites

### **1. Install Rust & Solana CLI**
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Add to PATH
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

# Verify installation
solana --version
cargo --version
```

### **2. Set HOME Environment Variable**
This is CRITICAL for `cargo build-sbf` on Windows:

```powershell
# Open PowerShell as Administrator!
$env:HOME = "$env:USERPROFILE"
echo $env:HOME
```

---

## 🚀 Step 1: Prepare the Repository

### **1.1 Navigate to Percolator Fork**
```bash
cd C:\Users\Rey\Desktop\percolator-fork
```

### **1.2 Verify Cargo.lock is Valid**
```bash
# Check lock file version
type Cargo.lock | find "version"

# Should show: version = 4
```

### **1.3 Clean Previous Builds**
```bash
cargo clean
```

---

## 🏗️ Step 2: Build Router Program

### **2.1 Run cargo build-sbf (NO --release flag)**
```bash
cargo build-sbf --package percolator_router
```

**Expected output:**
```
   Compiling percolator_router v0.1.0
    Finished sbf-solana-solana/debug profile [unoptimized + debuginfo] target(s) in 45s
    
    To deploy this program:
      $ solana program deploy ./target/sbf-solana-solana/debug/percolator_router.so
```

### **2.2 Verify Build Success**
```bash
ls target/sbf-solana-solana/debug/percolator_router.so
```

Should show: `percolator_router.so` exists

---

## 🏗️ Step 3: Build Slab Program

### **3.1 Build Slab Program**
```bash
cargo build-sbf --package percolator_slab
```

**Expected output:**
```
   Compiling percolator_slab v0.1.0
    Finished sbf-solana-solana/debug profile [unoptimized + debuginfo] target(s) in 45s
    
    To deploy this program:
      $ solana program deploy ./target/sbf-solana-solana/release/percolator_slab.so
```

### **3.2 Verify Build Success**
```bash
ls target/sbf-solana-solana/debug/percolator_slab.so
```

Should show: `percolator_slab.so` exists

---

## ✅ Troubleshooting: Common Errors

### **Error 1: "Can't get home directory path: environment variable not found"**

**Cause**: HOME environment variable not set on Windows

**Fix**:
```powershell
# In PowerShell as Administrator:
$env:HOME = "$env:USERPROFILE"
Write-Host "HOME is: $env:HOME"
```

Then retry: `cargo build-sbf --package percolator_router`

---

### **Error 2: "lock file version 4 was found, but this version of Cargo does not understand this lock file"**

**Cause**: Cargo version mismatch

**Fix**:
```bash
# Update Rust toolchain
rustup update

# Regenerate lock file
cargo update

# Clean and retry
cargo clean
cargo build-sbf --package percolator_router
```

---

### **Error 3: "the argument '--release' cannot be used multiple times"**

**Cause**: Using `-- --release` syntax (which is wrong)

**Fix**: 
```bash
# WRONG:
cargo build-sbf -- --release

# CORRECT:
cargo build-sbf --package percolator_router
```

For release builds, use:
```bash
# Builds in release mode automatically
cargo build-sbf --package percolator_router
```

---

### **Error 4: "Failed to install platform-tools: A required privilege is not held"**

**Cause**: Command needs admin privileges

**Fix**:
```powershell
# Open PowerShell as Administrator
# Then set HOME variable again:
$env:HOME = "$env:USERPROFILE"

# Then run the command
cd C:\Users\Rey\Desktop\percolator-fork
cargo build-sbf --package percolator_router
```

---

## 🚀 Step 4: Start Local Validator

### **4.1 Open New PowerShell Terminal as Administrator**

**IMPORTANT**: Do this in a **NEW terminal window**, keep it running

```powershell
# Set HOME variable
$env:HOME = "$env:USERPROFILE"

# Start validator (keep this running!)
solana-test-validator
```

**Expected output:**
```
Ledger location: C:\Users\Rey\.local\share\solana\test-ledger
Log: C:\Users\Rey\.local\share\solana\test-ledger\validator.log
⠈ Initializing...
✓ Initialized
...
Waiting for your transaction...
```

⚠️ **Keep this terminal OPEN and running!**

---

## 📋 Step 5: Deploy Programs (In Original Terminal)

### **5.1 Configure Solana CLI for Local Network**

In your **original terminal** (not the validator one):

```bash
# Configure for localhost
solana config set --url http://localhost:8899
solana config get
```

**Expected output:**
```
Config File: C:\Users\Rey\.config\solana\cli\config.yml
RPC URL: http://localhost:8899
WebSocket URL: ws://localhost:8900/
Keypair Path: C:\Users\Rey\.config\solana\cli\id.json
```

---

### **5.2 Airdrop SOL for Deployment**

```bash
# Get your address
solana address

# Output: Your address (e.g., xyz123...)

# Airdrop 2 SOL for deployment
solana airdrop 2

# Verify balance
solana balance
```

**Expected output:**
```
Requesting airdrop of 2 SOL
Signature: ABC...XYZ
2 SOL
```

---

### **5.3 Deploy Router Program**

```bash
cd C:\Users\Rey\Desktop\percolator-fork

solana program deploy target/sbf-solana-solana/debug/percolator_router.so
```

**Expected output:**
```
Program Id: RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr
```

✅ **Save this Program ID!**

---

### **5.4 Deploy Slab Program**

```bash
solana program deploy target/sbf-solana-solana/debug/percolator_slab.so
```

**Expected output:**
```
Program Id: SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk
```

✅ **Save this Program ID!**

---

## ✅ Step 6: Verify Deployment

### **6.1 Check Program State**

```bash
# Check Router
solana program show RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr

# Check Slab
solana program show SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk
```

**Expected output:**
```
Program Id: RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr
Owner: BPFLoaderUpgradeab1e11111111111111111111111
ProgramData Address: ...
Authority: Your Address
...
```

---

### **6.2 Test RPC Connection**

```bash
solana cluster-version
```

**Expected output:**
```
0.0.1 (unknown; hash: ...)
```

---

## 📊 Full Build & Deploy Checklist

- [ ] **Rust & Solana CLI installed** (`rustup --version`, `solana --version`)
- [ ] **HOME environment variable set** (`$env:HOME = "$env:USERPROFILE"`)
- [ ] **Percolator forked & cloned** locally
- [ ] **Cargo.lock valid** (version = 4)
- [ ] **Router built successfully** (`cargo build-sbf --package percolator_router`)
- [ ] **Slab built successfully** (`cargo build-sbf --package percolator_slab`)
- [ ] **Router .so file exists** (`target/sbf-solana-solana/debug/percolator_router.so`)
- [ ] **Slab .so file exists** (`target/sbf-solana-solana/debug/percolator_slab.so`)
- [ ] **Validator running** (in separate terminal)
- [ ] **CLI configured for localhost** (`solana config set --url http://localhost:8899`)
- [ ] **SOL airdropped** (at least 2 SOL)
- [ ] **Router deployed** (Program ID saved)
- [ ] **Slab deployed** (Program ID saved)
- [ ] **Programs verified** (`solana program show` works)

---

## 🎯 Next Steps

Once both programs are deployed:

1. **Update Dashboard Environment Variables**
   ```env
   NEXT_PUBLIC_ROUTER_PROGRAM=RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr
   NEXT_PUBLIC_SLAB_PROGRAM=SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk
   NEXT_PUBLIC_RPC_ENDPOINT=http://localhost:8899
   ```

2. **Run Unit Tests**
   ```bash
   cargo test --lib
   ```

3. **Create Integration Tests**
   - Test reserve-commit flow
   - Test order matching
   - Test liquidation

4. **Connect Dashboard**
   - Update `hooks/use-trading.ts` to call programs
   - Test wallet signing
   - Display on-chain state

---

## 📞 Quick Reference

| Command | Purpose |
|---------|---------|
| `cargo build-sbf --package percolator_router` | Build Router |
| `cargo build-sbf --package percolator_slab` | Build Slab |
| `solana-test-validator` | Start local validator |
| `solana config set --url http://localhost:8899` | Configure for local |
| `solana program deploy target/sbf-solana-solana/debug/percolator_router.so` | Deploy Router |
| `solana program deploy target/sbf-solana-solana/debug/percolator_slab.so` | Deploy Slab |
| `solana program show <PROGRAM_ID>` | Verify deployment |

---

## 🎉 Success Criteria

✅ Router .so file built  
✅ Slab .so file built  
✅ Router deployed to local validator  
✅ Slab deployed to local validator  
✅ Both programs verified with `solana program show`  
✅ Program IDs obtained and saved  

---

**Ready to build? Let's go!** 🚀

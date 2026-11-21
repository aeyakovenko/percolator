#include <llvm-c/Analysis.h>
#include <llvm-c/Core.h>
#include <llvm-c/IRReader.h>
#include <llvm-c/Transforms/PassBuilder.h>

#include <iostream>
#include <string>

// These are standard LLVM optimizations. The only added attribute requests
// inlining; LLVM derives its own memory effects from the actual callees.
static bool reduce(LLVMContextRef ctx, LLVMModuleRef module) {
    auto kind = LLVMGetEnumAttributeKindForName("alwaysinline", 12);
    auto hint = LLVMCreateEnumAttribute(ctx, kind, 0);
    for (auto f = LLVMGetFirstFunction(module); f; f = LLVMGetNextFunction(f)) {
        size_t length;
        auto raw = LLVMGetValueName2(f, &length);
        std::string name(raw, length);
        if (!LLVMIsDeclaration(f) && (name.find("_ZN10percolator") != std::string::npos || name.find("_10percolator") != std::string::npos) &&
            name.find("14validate_shape") == std::string::npos)
            LLVMAddAttributeAtIndex(f, LLVMAttributeFunctionIndex, hint);
    }
    const char *passes =
        "always-inline,function(sroa,early-cse,instcombine<max-iterations=12>,simplifycfg),"
        "cgscc(function-attrs),function(early-cse<memssa>,gvn,instcombine<max-iterations=12>,simplifycfg)";
    auto options = LLVMCreatePassBuilderOptions();
    LLVMErrorRef err = LLVMRunPasses(module, passes, nullptr, options);
    LLVMDisposePassBuilderOptions(options);
    if (!err) return true;
    char *message = LLVMGetErrorMessage(err);
    std::cerr << message << '\n';
    LLVMDisposeErrorMessage(message);
    return false;
}

static bool allReturnsFalse(LLVMModuleRef module, const char *name) {
    auto fn = LLVMGetNamedFunction(module, name);
    if (!fn || LLVMIsDeclaration(fn)) return false;
    unsigned returns = 0;
    for (auto b = LLVMGetFirstBasicBlock(fn); b; b = LLVMGetNextBasicBlock(b)) {
        auto term = LLVMGetBasicBlockTerminator(b);
        if (LLVMGetInstructionOpcode(term) != LLVMRet) continue;
        if (LLVMGetNumOperands(term) != 1) return false;
        auto v = LLVMGetOperand(term, 0);
        if (!LLVMIsAConstantInt(v) || LLVMGetIntTypeWidth(LLVMTypeOf(v)) != 1 ||
            LLVMConstIntGetZExtValue(v) != 0) return false;
        ++returns;
    }
    return returns > 0;
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    auto ctx = LLVMContextCreate();
    LLVMMemoryBufferRef buf;
    LLVMModuleRef mod;
    char *err = nullptr;
    if (LLVMCreateMemoryBufferWithContentsOfFile(argv[1], &buf, &err) ||
        LLVMParseIRInContext(ctx, buf, &mod, &err) ||
        LLVMVerifyModule(mod, LLVMReturnStatusAction, &err)) {
        std::cerr << err << '\n';
        return 2;
    }
    if (!reduce(ctx, mod)) return 2;
    if (LLVMVerifyModule(mod, LLVMReturnStatusAction, &err) ||
        LLVMPrintModuleToFile(mod, argv[2], &err)) {
        std::cerr << err << '\n';
        return 2;
    }
    bool both = true;
    for (const char *name : {"positive_counterexample", "zero_counterexample"}) {
        bool ok = allReturnsFalse(mod, name);
        std::cout << name << '=' << (ok ? "all_false" : "unresolved") << '\n';
        both &= ok;
    }
    return both ? 0 : 1;
}

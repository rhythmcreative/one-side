// One-Side: loads a .metallib written by One-Side's xcrun (Metal source and
// its options) with the Metal compiler macOS has built in, so FSR 4 runs
// without Xcode's Metal toolchain. Real metallibs load as before.
#pragma once
#ifdef __OBJC__
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <string.h>

static inline id<MTLLibrary> oneside_library(id<MTLDevice> device, NSURL *url, NSError **error) {
    static const char magic[] = "ONESIDE-MSL1\n";
    const NSUInteger n = sizeof magic - 1;
    NSData *data = [NSData dataWithContentsOfURL:url options:NSDataReadingMappedIfSafe error:nil];
    if (!data || data.length <= n || memcmp(data.bytes, magic, n) != 0)
        return [device newLibraryWithURL:url error:error];
    NSDictionary *unit = [NSJSONSerialization JSONObjectWithData:[data subdataWithRange:NSMakeRange(n, data.length - n)]
                                                         options:0 error:error];
    if (![unit isKindOfClass:NSDictionary.class]) return nil;
    MTLCompileOptions *options = [MTLCompileOptions new];
    NSString *std = unit[@"std"];
    NSDictionary<NSString *, NSNumber *> *versions = @{
        @"macos-metal2.0": @(MTLLanguageVersion2_0), @"metal2.0": @(MTLLanguageVersion2_0),
        @"macos-metal2.1": @(MTLLanguageVersion2_1), @"metal2.1": @(MTLLanguageVersion2_1),
        @"macos-metal2.2": @(MTLLanguageVersion2_2), @"metal2.2": @(MTLLanguageVersion2_2),
        @"macos-metal2.3": @(MTLLanguageVersion2_3), @"metal2.3": @(MTLLanguageVersion2_3),
        @"macos-metal2.4": @(MTLLanguageVersion2_4), @"metal2.4": @(MTLLanguageVersion2_4),
        @"metal3.0": @(MTLLanguageVersion3_0), @"metal3.1": @(MTLLanguageVersion3_1),
        @"metal3.2": @(MTLLanguageVersion3_2), @"metal4.0": @(MTLLanguageVersion4_0),
    };
    if (versions[std]) options.languageVersion = (MTLLanguageVersion)versions[std].unsignedIntegerValue;
    NSString *math = unit[@"math"];
    if ([math isEqualToString:@"fast"]) options.mathMode = MTLMathModeFast;
    else if ([math isEqualToString:@"relaxed"]) options.mathMode = MTLMathModeRelaxed;
    else if ([math isEqualToString:@"safe"]) options.mathMode = MTLMathModeSafe;
    NSString *fp32 = unit[@"fp32"];
    if ([fp32 isEqualToString:@"fast"]) options.mathFloatingPointFunctions = MTLMathFloatingPointFunctionsFast;
    else if ([fp32 isEqualToString:@"precise"]) options.mathFloatingPointFunctions = MTLMathFloatingPointFunctionsPrecise;
    NSString *opt = unit[@"opt"];
    if ([opt isEqualToString:@"0"] || [opt isEqualToString:@"s"] || [opt isEqualToString:@"z"])
        options.optimizationLevel = MTLLibraryOptimizationLevelSize;
    NSDictionary *defines = unit[@"defines"];
    if (defines.count) options.preprocessorMacros = defines;
    NSError *compileError = nil;
    id<MTLLibrary> lib = [device newLibraryWithSource:unit[@"source"] options:options error:&compileError];
    if (error) *error = lib ? nil : compileError;  // warnings come back as an error on success
    return lib;
}
#endif

// One-Side's Vulkan driver shim. The Vulkan loader opens it in place of KosmicKrisp
// (VK_DRIVER_FILES names its manifest); it opens the engine's own KosmicKrisp and
// hands every Vulkan entry point over to it unchanged.
//
// First: Metal's shader compiler stops at 256 nested brackets, and
// the MSL that KosmicKrisp writes for large D3D12 shaders nests control flow deeper
// than that (A Plague Tale: Requiem has a compute shader 390 blocks deep), so the
// pipeline fails and the game draws black. Metal passes MTLCompileOptions'
// additionalCompilerArguments to its compiler, and the compiler takes
// "-fbracket-depth N", so every source compile in the process gets a higher limit.
//
// Second: such shaders are huge (95 MB of MSL for that one), and one game asks for
// several at once. Metal compiles all of a process's libraries in one compiler
// service, which runs out of memory with a few of them in flight and dies
// (XPC_ERROR_CONNECTION_INTERRUPTED), failing every one. Sources over BIG_SOURCE are
// compiled one at a time; small ones, nearly all of them, are not held back.
// Metal caches what it compiled, so only a game's first start waits for them.
// While any is pending, the file compiling-<pid> in ONESIDE_STATUS_DIR holds how many,
// so One-Side can say the black screen is shaders being prepared.

#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <objc/runtime.h>
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <limits.h>
#include <pthread.h>
#include <unistd.h>

#define DEPTH_ARG @"-fbracket-depth 2048"

#define BIG_SOURCE (4u << 20)
#define BIG_JOBS 1

static void *real;
static IMP orig_arguments;
static IMP orig_new_library;
static dispatch_semaphore_t big_compile;
static pthread_mutex_t status_lock = PTHREAD_MUTEX_INITIALIZER;
static int pending;
static char status_path[PATH_MAX];

static void count_pending(int delta) {
    pthread_mutex_lock(&status_lock);
    pending += delta;
    if (*status_path) {
        if (pending > 0) {
            FILE *f = fopen(status_path, "w");
            if (f) {
                fprintf(f, "%d\n", pending);
                fclose(f);
            }
        } else {
            unlink(status_path);
        }
    }
    pthread_mutex_unlock(&status_lock);
}

API_AVAILABLE(macos(26.0)) static id new_library(id self, SEL cmd, MTL4LibraryDescriptor *desc, NSError **error) {
    BOOL big = desc.source.length > BIG_SOURCE;
    if (big) {
        count_pending(1);
        dispatch_semaphore_wait(big_compile, DISPATCH_TIME_FOREVER);
    }
    id lib = ((id (*)(id, SEL, MTL4LibraryDescriptor *, NSError **)) orig_new_library)(self, cmd, desc, error);
    if (big) {
        dispatch_semaphore_signal(big_compile);
        count_pending(-1);
    }
    return lib;
}

static NSString *arguments(id self, SEL cmd) {
    NSString *a = ((NSString * (*)(id, SEL)) orig_arguments)(self, cmd);
    if ([a containsString:@"-fbracket-depth"]) return a;
    return a.length ? [a stringByAppendingFormat:@" %@", DEPTH_ARG] : DEPTH_ARG;
}

__attribute__((constructor)) static void onesidekk_init(void) {
    @autoreleasepool {
        Class c = object_getClass([MTLCompileOptions new]);
        Method m = class_getInstanceMethod(c, @selector(additionalCompilerArguments));
        if (m) orig_arguments = method_setImplementation(m, (IMP)arguments);
        else fprintf(stderr, "onesidekk: MTLCompileOptions has no additionalCompilerArguments; shaders keep Metal's limit\n");
        // KosmicKrisp compiles through an MTL4Compiler; its class is the GPU driver's.
        if (@available(macOS 26.0, *)) {
            const char *jobs = getenv("ONESIDE_BIG_SHADER_JOBS");
            big_compile = dispatch_semaphore_create(jobs && atoi(jobs) > 0 ? atoi(jobs) : BIG_JOBS);
            id<MTLDevice> dev = MTLCreateSystemDefaultDevice();
            id compiler = [dev newCompilerWithDescriptor:[MTL4CompilerDescriptor new] error:nil];
            Method n = compiler ? class_getInstanceMethod(object_getClass(compiler), @selector(newLibraryWithDescriptor:error:)) : NULL;
            if (n) orig_new_library = method_setImplementation(n, (IMP)new_library);
            else fprintf(stderr, "onesidekk: no MTL4Compiler; huge shaders are not serialized\n");
        }
    }
    const char *dir = getenv("ONESIDE_STATUS_DIR");
    if (dir) snprintf(status_path, sizeof(status_path), "%s/compiling-%d", dir, getpid());
    const char *path = getenv("ONESIDE_VK_DRIVER");
    if (path) real = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!real) fprintf(stderr, "onesidekk: cannot open KosmicKrisp at %s: %s\n", path ? path : "(ONESIDE_VK_DRIVER unset)", dlerror());
}

typedef void (*vk_void_fn)(void);

static void *sym(const char *name) {
    return real ? dlsym(real, name) : NULL;
}

__attribute__((visibility("default"))) int32_t vk_icdNegotiateLoaderICDInterfaceVersion(uint32_t *version) {
    int32_t (*f)(uint32_t *) = sym("vk_icdNegotiateLoaderICDInterfaceVersion");
    return f ? f(version) : -9; // VK_ERROR_INCOMPATIBLE_DRIVER
}

__attribute__((visibility("default"))) vk_void_fn vk_icdGetInstanceProcAddr(void *instance, const char *name) {
    vk_void_fn (*f)(void *, const char *) = sym("vk_icdGetInstanceProcAddr");
    return f ? f(instance, name) : NULL;
}

__attribute__((visibility("default"))) vk_void_fn vk_icdGetPhysicalDeviceProcAddr(void *instance, const char *name) {
    vk_void_fn (*f)(void *, const char *) = sym("vk_icdGetPhysicalDeviceProcAddr");
    return f ? f(instance, name) : NULL;
}

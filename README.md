# Zig to QuakeC compiler

This is a project to allow writing Quake mods in Zig instead of QuakeC. If you know anything about QuakeC, this will sound like a completely unhinged idea. That's because it is. [See below](#how-it-works) for a description of how I got it to work.

## Prerequisites

If you want to run the full toolchain, you'll need 3 tools:

- Zig: follow the installation instructions on [their website](https://ziglang.org/).
- `wasm-opt`: part of the [Binaryen](https://github.com/WebAssembly/binaryen) project.
- FTEQCC: for Windows and Linux, you can download it from [their website](https://www.fteqcc.org/), for macOS you'll need to use [@BryanHaley's fork](https://github.com/BryanHaley/fteqcc-mac).

  In theory, this project should work with any other QuakeC compiler (even the vanilla one), but it looks for a binary called `fteqcc` and hasn't been tested with any other compilers.

## Installation

```sh
cargo install --git https://github.com/eira-fransham/zig-qc.git
```

This will install the `zig-qc` binary.

## Usage

To start, create a new folder and run `zig-qc init` inside it. This works basically the same as `zig init`, except that it uses a project template that allows you to compile to QuakeC. See `zig-src` for an example.

```sh
mkdir my-new-mod
cd my-new-mod
zig-qc init
```

By default, this generates a project targeting vanilla Quake's `defs.qc`. If you want to use QuakeWorld's `defs.qc`, use the `--target` flag:

```sh
zig-qc init --target quakeworld
```

If you want to manually specify the project name instead of using the name of the current directory, just pass it as an extra argument:

```sh
mkdir my-new-mod
cd my-new-mod
zig-qc init mod-with-a-custom-name
```

The template creates a new `root.zig` that exports a main function like so:

```zig
const defs = @import("defs.zig");

pub export fn main() void {
    defs.dprint(defs.String.lit("hello from my_new_mod\n"));
}
```

From there, you can start writing your mod! To see a more complex example see the `qw-zig` folder, or check out [my fork of LibreQuake](https://github.com/eira-fransham/LibreQuake-zig) which includes a translation of LibreQuake's QuakeC code to Zig that gets compiled to a vanilla-compatible binary.

## Comparison to QuakeC

I intended this project to just be a toy, but Zig turns out to be a pretty nice language for writing QuakeC-style code. LLVM's optimiser is (unsuprisingly) _really_ good, and all the game code I've tried has compiled to smaller bytecode when written in Zig and compiled with `zig-qc build` than when written by hand in QuakeC. Zig is far more convenient to use. It has a language server, which means _finally_ getting proper autocomplete and live error messages while writing code, and Zig's [`comptime` feature](https://ziglang.org/documentation/master/#comptime) makes it way easier to write helper functions. A lot of code which is pretty ugly in QuakeC can be simplified significantly when written in Zig. For example, here's a snippet from the original Quake's game code:

```quakec
void() func_button =
{
  // ..
  if (self.sounds == 0)
  {
    precache_sound ("buttons/airbut1.wav");
    self.noise = "buttons/airbut1.wav";
  }
  if (self.sounds == 1)
  {
    precache_sound ("buttons/switch21.wav");
    self.noise = "buttons/switch21.wav";
  }
  if (self.sounds == 2)
  {
    precache_sound ("buttons/switch02.wav");
    self.noise = "buttons/switch02.wav";
  }
  if (self.sounds == 3)
  {
    precache_sound ("buttons/switch04.wav");
    self.noise = "buttons/switch04.wav";
  }
  // ..
}
```

In Zig, this could look like this:

```zig
fn set_sound(
    e: entity,
    comptime dir: []const u8,
    comptime options: []const []const u8,
) void {
    inline for (0..options.len) |i| {
        if (self.sounds == i) {
            const sound = S(dir ++ "/" ++ button_sounds[i]);
            _ = q.precache_sound(sound);
            self.noise = sound;
        }
    }
}

export fn func_button() void {
    // ..
    set_sound(
        self,
        "buttons",
        &.{
            "airbut1.wav",
            "switch21.wav",
            "switch02.wav",
            "switch04.wav",
        },
    )
    // ..
}
```

These compile to identical code. Here's an explanation of the `set_sound` helper function, in case you're new to Zig:

```zig
// `[]const u8` is a Zig string, `[]const []const u8` is an
// array of strings. Both `dir` and `options` are `comptime`
// so we can treat them like we would string literals, using
// things like `++` for concatenation.
fn set_sound(
    e: entity,
    comptime dir: []const u8,
    comptime options: []const []const u8,
) void {
    // `inline for` ensures that `i` is a compile-time
    // constant, so anything using it will get compiled out
    inline for (0..options.len) |i| {
        // We need to check for equality and use `i` inside
        // the block, since `self.sounds` isn't a
        // compile-time constant.
        //
        // Usually we could write `if (self.sound) continue`
        // but `continue` doesn't work inside `inline for`
        // in the current version of Zig.
        if (self.sounds == i) {
            // `S` is a convenience alias for `String.lit`
            const sound = S(dir ++ "/" ++ button_sounds[i]);
            // `precache_sound` returns a string for some
            // reason, even in the original Quake `defs.qc`
            _ = q.precache_sound(sound);
            self.noise = sound;
        }
    }
}
```

## How it works

There are two core ideas behind this project. First, WebAssembly is used as an intermediate translation layer. The Zig is compiled to WebAssembly, that WebAssembly is compiled to QuakeC source code, and then that source code is compiled with FTEQCC (or any other QuakeC compiler). The issue with doing this is that once we compile to WebAssembly we lose a lot of information. Most importantly, we lose types. However, we can work around this.

### How it works: literals

All numerical and logical operations in QuakeC are handled with floats, so anything that we use numerical operations on can default to being a float. For vectors, we can use an explicit SIMD type inside Zig, which get converted to WebAssembly's [`v128`](https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/Value_types/v128) type which we can detect.

The only way for QuakeC to construct a `string` is as a literal. We have a function which constructs a `string` from a `comptime []const u8` called `String.lit`, which ultimately compiles to a call to an `extern fn`. When reading the WebAssembly, we can figure out what pointer was passed to that `extern fn` and look in the `data` section of the binary to figure out what string we should emit to the QuakeC file.

`entity`s can't be constructed from QuakeC at all, only returned from builtins or read from the engine via globals and fields. To understand how we can use that information, we need to talk about type inference.

> Originally my plan was to use [`externref`](https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/Value_types/externref) for entities and strings (which are opaque to QuakeC), but sadly due to [a bug in the version of LLVM used by Zig](https://github.com/llvm/llvm-project/issues/69894) this was impossible.

### How it works: type inference

The most important part of this project is global type inference. If you've used any language with type inference, such as Zig itself, you'll know that there are cases where the compiler errors out because it can't figure out what type a value is supposed to be. In our case, that's unacceptable.

Since a WebAssembly binary is entirely self-contained and we have a small number of non-generic types that any value could be, we can conduct global type inference on the entire binary. First, the binary is converted to [SSA form](https://en.wikipedia.org/wiki/Static_single-assignment_form) using the fantastic [`waffle`](https://github.com/bytecodealliance/waffle) library. This means that it's impossible for types to change under our nose. A WebAssembly `i32` could represent a `string` or an `entity`[^wasm-i32], so a local `i32` variable could be used for a `string` at one point in the function and an `entity` at another point. SSA form means we know that every variable has precisely one type. Well, almost every variable. To explain what I mean by "almost", let's get into functions.

[^wasm-i32]: An `i32` could even represent a `float`, since bitwise operations are done on floats in QuakeC but ints in Zig. It actually gets even more complex than that, but covering that would make this explanation even longer than it already is.

### How it works: functions

It's not uncommon to end up wth two functions that have the same body. This is especially true when compiling Zig, since the compilation goes via LLVM and even functions which didn't start out with the same body could end up looking the same after optimisation. The most basic example of two functions with the same body looks like this:

```quakec
string(string v) identity_s =
{
    return v;
}

entity(entity v) identity_e =
{
    return v;
}
```

Once compiled to WebAssembly, these will both be functions that simply take an `i32` and return it unmodified, which get helpfully deduplicated into a single function. Even if we use our global type analysis to figure out the argument types of every function, we still might end up with functions which have multiple possible argument types. However, the type checker keeps all possible candidates in mind, and if a function has multiple potential types by the time we emit the QuakeC code, we just split it into one function per set of types[^fteqcc-variant].

[^fteqcc-variant]: FTEQCC supports `__variant`, which can be any type. For our `identity` example, that would work just fine. Unfortunately, you can't actually do anything with a value of type `__variant` other than pass it around, so we'd still need to figure out a way to deduplicate functions that wouldn't support variant. I also try to generate relatvely nice-looking QuakeC code which is compatible with any compiler if I can, so I really didn't want to use `__variant` if I could avoid it.

### How it works: globals and fields

Globals are basically just accesses to a known pointer offset in global memory. We can figure out what address a memory read is trying to access and map that to a global from the `defs.qc`, since the compiler knows what globals already exist. Fields are the same - we store `entity`s as pointers, so any time we find a pointer offset from an `entity` then we can convert that to accessing a field.

### How it works: stack variables

LLVM will try to keep values in locals, which are easy for us to figure out the types for. However, in some cases it will "spill" values to the so-called "shadow stack", which works much the same as stack memory in any language. Pointers to the stack look the same as any pointer, which is a problem since an `entity` is just a pointer and reading a `global` is just a pointer

## Other features

The compiler has a few extra passes that exist to make the output a bit nicer-looking. During analysis, all control flow is essentially treated as `goto`, so the converter tries to detect common patterns like `while(cond) { .. }`, `do { .. } while(cond);`, nested if-else blocks, `switch` statements etc. This should hopefully be optimised better by FTEQCC than using `goto`s. I originally implemented this because using `if (cond) { goto label; }` caused FTEQCC to hang forever.

## Testing

There are fairly-extensive snapshot tests to make sure that it's obvious if a compiler change resulted in different code. I also use the [`run-qc` command](https://github.com/eira-fransham/bevy_mod_scripting_qcvm/tree/master/crates/run-qc) from my [`qcvm` project](https://github.com/eira-fransham/bevy_mod_scripting_qcvm/tree/master/crates/qcvm) to ensure that the generated code compiles correctly.

I've also tested `progs.dat` compiled from [my Zig translation of LibreQuake](https://github.com/eira-fransham/LibreQuake-zig/tree/main/lq-zig) in both FTEQW and my own [Seismon engine](https://github.com/eira-fransham/seismon) and haven't noticed any differences.

## Limitations

Currently, we only detect a limited number of ways that the stack can be used. The biggest limitation is that functions taking a pointer to a value that lives on the stack are not supported at all. There are ways we could support this, but right now we just return an error. For now, just assume that function arguments and return values have the same limitations as they do in QuakeC code, i.e. just bare `float`, `vector`, `string` or `entity`.

We also don't try to support FTEQCC's `int` type, although it would simplify some things. I didn't know exactly how FTEQCC handled `int` when generating vanilla-compatible `progs.dat`, so instead of figuring it out I just kicked that particular can down the road.

---

## AI disclosure

This README was written by hand, but the project was built with a lot of AI assistance. In particular, the Zig port of both QuakeWorld's and LibreQuake's QuakeC code were entirely handled by AI (albeit with a lot of human direction). The tests and implementation was written with AI assistance, and AI was used to debug issues..

I haven't manually written any documentation yet other than this README, so please take into account that most of the code documentation and comments you see have been written by the AI. While I've looked over all the code now, and I've removed any obviously-wrong documentation/comments, I haven't written any of my own just yet. Take anything you read with a grain of salt.

The original plan with this project was for it to be a silly little task that I could use as a real-world example to see how vibe-coding worked. I recently started trying out Claude, so the idea was that I would come up with the general mechanism and let the LLM do the rest, only reviewing its plans. However, it didn't take long before the parts of the code I did see were too obviously-wrong for me to accept, so I ended up redesigning the project myself and just let Claude handle debugging and filling out the implementation.

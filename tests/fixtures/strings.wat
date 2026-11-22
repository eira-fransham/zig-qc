(module
  (import "env" "memory" (memory 1))
  (import "env" "__memory_base" (global $base i32))
  (import "env" "__quakec_string_literal" (func $lit (param i32 i32) (result i64)))
  (import "builtin" "dprint" (func $dprint (param i64)))
  ;; +0:  every escape class, then a hex digit right after a `\xHH` escape
  ;;      (25 bytes)
  ;; +25: "yes", +28: "no" (no NUL terminators: lengths are explicit)
  ;; +30: a u8 table of 3 entries, read dynamically
  (data (global.get $base)
    "quote\22 bs\5c nl\0a tab\09 \01\80\ffAB"
    "yesno"
    "\07\08\09")
  ;; Escapes, and the base address kept in a local as LLVM often does.
  (func (export "escapes")
    (local $b i32)
    (call $dprint (call $lit (local.tee $b (global.get $base)) (i32.const 25)))
    (call $dprint (call $lit (i32.add (local.get $b) (i32.const 25)) (i32.const 3))))
  ;; An empty literal may have any address.
  (func (export "empty") (result i64)
    (call $lit (i32.const 0) (i32.const 0)))
  ;; A literal chosen at runtime.
  (func (export "choose") (param $x i32) (result i64)
    (call $lit
      (select
        (i32.add (global.get $base) (i32.const 25))
        (i32.add (global.get $base) (i32.const 28))
        (local.get $x))
      (select (i32.const 3) (i32.const 2) (local.get $x))))
  ;; A dynamic load: the accessor leaves out string bytes.
  (func (export "table") (param $i i32) (result f32)
    (f32.convert_i32_u
      (i32.load8_u offset=30 (i32.add (global.get $base) (local.get $i))))))

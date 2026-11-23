(module
  (import "env" "memory" (memory 1))
  (import "env" "__memory_base" (global $base i32))
  (import "env" "__quakec_string_literal" (func $lit (param i32) (result i32)))
  (import "builtin" "dprint" (func $dprint (param i32)))
  ;; +0:  every escape class, then a hex digit right after a `\xHH` escape
  ;;      (25 bytes and a NUL)
  ;; +26: "yes", +30: "no", +33: ""
  ;; +34: a u8 table of 3 entries, read dynamically
  (data (global.get $base)
    "quote\22 bs\5c nl\0a tab\09 \01\80\ffAB\00"
    "yes\00no\00\00"
    "\07\08\09")
  ;; Escapes, and the base address kept in a local as LLVM often does.
  (func (export "escapes")
    (local $b i32)
    (call $dprint (call $lit (local.tee $b (global.get $base))))
    (call $dprint (call $lit (i32.add (local.get $b) (i32.const 26)))))
  (func (export "empty") (result i32)
    (call $lit (i32.add (global.get $base) (i32.const 33))))
  ;; A literal chosen at runtime.
  (func (export "choose") (param $x i32) (result i32)
    (call $lit
      (select
        (i32.add (global.get $base) (i32.const 26))
        (i32.add (global.get $base) (i32.const 30))
        (local.get $x))))
  ;; A dynamic load: the accessor leaves out string bytes.
  (func (export "table") (param $i i32) (result f32)
    (f32.convert_i32_u
      (i32.load8_u offset=34 (i32.add (global.get $base) (local.get $i))))))

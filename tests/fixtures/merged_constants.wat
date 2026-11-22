;; Values LLVM computes on different paths and merges: function pointers,
;; string literal addresses and global addresses that are one of several
;; constants.
(module
  (import "env" "memory" (memory 1))
  (import "env" "__indirect_function_table" (table 2 funcref))
  (import "env" "__memory_base" (global $mb i32))
  (import "env" "__table_base" (global $tb i32))
  ;; wasm-ld may import this even if nothing uses it.
  (import "env" "__stack_pointer" (global $sp (mut i32)))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i64)))
  (import "env" "__quakec_func_ref" (func $func_ref (param i32) (result funcref)))
  (import "env" "__quakec_store_entity_field_fn" (func $store_fn (param i64 i32 funcref)))
  (import "env" "__quakec_string_literal" (func $lit (param i32 i32) (result i64)))
  (import "env" "__quakec_load_float" (func $load_float (param i32) (result f32)))
  (import "builtin" "dprint" (func $dprint (param i64)))
  (elem (global.get $tb) func $a $b)
  (data (global.get $mb) "on\00no\00")
  (func $a)
  (func $b)
  ;; `self.think = c ? a : b`, merged into one store of a block result.
  (func (export "set_think") (param $c i32)
    (local $f i32)
    (local.set $f
      (block (result i32)
        (drop (br_if 0 (i32.add (global.get $tb) (i32.const 0)) (local.get $c)))
        (i32.add (global.get $tb) (i32.const 1))))
    (call $store_fn (call $load_entity (i32.const 112)) (i32.const 44)
      (call $func_ref (local.get $f))))
  ;; `dprint(c ? "no" : "on")`: the base plus a `select` between offsets.
  (func (export "say") (param $c i32)
    (call $dprint (call $lit
      (i32.add (global.get $mb) (select (i32.const 3) (i32.const 0) (local.get $c)))
      (i32.const 2))))
  ;; The same, with the address assigned on two paths.
  (func (export "say_if") (param $c i32)
    (local $p i32)
    (if (local.get $c)
      (then (local.set $p (i32.add (global.get $mb) (i32.const 3))))
      (else (local.set $p (i32.add (global.get $mb) (i32.const 0)))))
    (call $dprint (call $lit (local.get $p) (i32.const 2))))
  ;; `c ? time : frametime`, as a load of a selected address.
  (func (export "pick_time") (param $c i32) (result f32)
    (call $load_float (select (i32.const 124) (i32.const 128) (local.get $c)))))

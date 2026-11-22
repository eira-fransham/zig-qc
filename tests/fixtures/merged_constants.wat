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
  (import "env" "__quakec_store_entity_field_fn" (func $store_fn (param i64 i32 i32)))
  (import "env" "__quakec_string_literal" (func $lit (param i32) (result i64)))
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
      (local.get $f)))
  ;; `dprint(c ? "no" : "on")`: the base plus a `select` between offsets.
  (func (export "say") (param $c i32)
    (call $dprint (call $lit
      (i32.add (global.get $mb) (select (i32.const 3) (i32.const 0) (local.get $c))))))
  ;; The same, with the address assigned on two paths.
  (func (export "say_if") (param $c i32)
    (local $p i32)
    (if (local.get $c)
      (then (local.set $p (i32.add (global.get $mb) (i32.const 3))))
      (else (local.set $p (i32.add (global.get $mb) (i32.const 0)))))
    (call $dprint (call $lit (local.get $p))))
  ;; `c ? time : frametime`, as a load of a selected address.
  (func (export "pick_time") (param $c i32) (result f32)
    (call $load_float (select (i32.const 124) (i32.const 128) (local.get $c))))
  ;; A local that holds a computed value elsewhere (wasm locals are reused),
  ;; but only literal addresses where it is used as one.
  (func (export "reused_local") (param $c i32) (result i32)
    (local $p i32)
    (local.set $p (i32.eq (local.get $c) (i32.const 7)))
    (if (local.get $p)
      (then (local.set $p (i32.add (global.get $mb) (i32.const 3))))
      (else (local.set $p (i32.add (global.get $mb) (i32.const 0)))))
    (call $dprint (call $lit (local.get $p)))
    (local.set $p (i32.mul (local.get $c) (i32.const 2)))
    (local.get $p))
  ;; Constants that reach the end of a block through a branch and the
  ;; fallthrough.
  (func (export "br_merge") (param $c i32)
    (local $f i32)
    (block
      (local.set $f (i32.add (global.get $tb) (i32.const 0)))
      (br_if 0 (local.get $c))
      (local.set $f (i32.add (global.get $tb) (i32.const 1))))
    (call $store_fn (call $load_entity (i32.const 112)) (i32.const 44)
      (local.get $f)))
  ;; A constant assigned in a loop, which stays known after it.
  (func (export "loop_forgets") (param $c i32)
    (local $f i32)
    (loop
      (local.set $f (i32.add (global.get $tb) (i32.const 1)))
      (br_if 0 (local.get $c)))
    (call $store_fn (call $load_entity (i32.const 112)) (i32.const 44)
      (local.get $f)))
  ;; `c ? b : a` for adjacent functions, as the table base plus a comparison
  ;; result.
  (func (export "bool_offset") (param $c f32)
    (call $store_fn (call $load_entity (i32.const 112)) (i32.const 44)
      (i32.add (global.get $tb) (f32.ne (local.get $c) (f32.const 0)))))
  ;; The same with a number: `5 - (x == 0)`.
  (func (export "bool_offset_number") (param $x i32) (result i32)
    (i32.sub (i32.const 5) (i32.eqz (local.get $x)))))

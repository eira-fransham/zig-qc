;; Function pointers and string literals passed as parameters: functions
;; merged by Binaryen's `merge-similar-functions` (which turns the constants
;; they differ in into parameters), and ordinary non-inlined functions.
(module
  (import "env" "memory" (memory 1))
  (import "env" "__indirect_function_table" (table 2 funcref))
  (import "env" "__memory_base" (global $mb i32))
  (import "env" "__table_base" (global $tb i32))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i64)))
  (import "env" "__quakec_store_entity_field_fn" (func $store_fn (param i64 i32 i32)))
  (import "env" "__quakec_store_entity_field_float" (func $store_float (param i64 i32 f32)))
  (import "env" "__quakec_string_literal" (func $lit (param i32) (result i64)))
  (import "env" "__quakec_load_entity_field_float" (func $load_float (param i64 i32) (result f32)))
  (import "builtin" "dprint" (func $dprint (param i64)))
  (elem (global.get $tb) func $a $b)
  (data (global.get $mb) "on\00no\00speed2\00")
  (func $a)
  (func $b)
  ;; Merged: `self.think = f; self.nextthink = n;`, with the function's
  ;; table offset as a parameter.
  (func $set_think (param $f i32) (param $n f32)
    (local $self i64)
    (local.set $self (call $load_entity (i32.const 112)))
    (call $store_fn (local.get $self) (i32.const 44)
      (i32.add (global.get $tb) (local.get $f)))
    (call $store_float (local.get $self) (i32.const 46) (local.get $n)))
  (func (export "think_a") (return_call $set_think (i32.const 0) (f32.const 0.1)))
  (func (export "think_b") (return_call $set_think (i32.const 1) (f32.const 0.2)))
  ;; Merged: a string literal whose address is a parameter.
  (func $say (param $p i32)
    (call $dprint (call $lit (i32.add (global.get $mb) (local.get $p)))))
  (func (export "say_on") (call $say (i32.const 0)))
  (func (export "say_no") (call $say (i32.const 3)))
  ;; Merged: an entity field offset (here of a user field, named in static
  ;; data) as a parameter. The base address is copied into a local first.
  (func $copy_field (param $fld i32)
    (local $self i64) (local $base i32)
    (local.set $self (call $load_entity (i32.const 112)))
    (local.set $base (global.get $mb))
    (call $store_float (local.get $self) (i32.const 46)
      (call $load_float (local.get $self) (i32.add (local.get $base) (local.get $fld)))))
  (func (export "copy_speed2") (call $copy_field (i32.const 6)))
  ;; Merged: a string literal address copied into a local and used twice,
  ;; and a number.
  (func $say_twice (param $len i32) (param $p i32)
    (local $a i32)
    (local.set $a (i32.add (global.get $mb) (local.get $p)))
    (call $dprint (call $lit (local.get $a)))
    (call $dprint (call $lit (local.get $a)))
    (call $store_float (call $load_entity (i32.const 112)) (i32.const 46)
      (f32.convert_i32_s (local.get $len))))
  (func (export "say_twice_on") (call $say_twice (i32.const 2) (i32.const 0)))
  (func (export "say_twice_no") (call $say_twice (i32.const 2) (i32.const 3)))
  ;; Not inlined: a function pointer passed as is.
  (func $set_think_ptr (param $f i32)
    (call $store_fn (call $load_entity (i32.const 112)) (i32.const 44)
      (local.get $f)))
  ;; Passes its parameter on, with an offset.
  (func $set_think_next (param $f i32)
    (call $set_think_ptr (i32.add (local.get $f) (i32.const 1))))
  (func (export "think_ptr") (param $c i32)
    (call $set_think_ptr (select (i32.add (global.get $tb) (i32.const 0))
      (i32.add (global.get $tb) (i32.const 1)) (local.get $c)))
    (call $set_think_next (global.get $tb))))

;; Values whose QuakeC type comes from how they are used rather than their
;; wasm type, and functions used at several types.
(module
  (type $void (func))
  (type $void_float (func (param f32)))
  (import "env" "memory" (memory 1))
  (import "env" "__indirect_function_table" (table 1 funcref))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i64)))
  (import "env" "__quakec_store_entity" (func $store_entity (param i32 i64)))
  (import "env" "__quakec_load_string" (func $load_string (param i32) (result i64)))
  (import "env" "__quakec_load_entity_field_fn" (func $load_fn (param i64 i32) (result i32)))
  (import "builtin" "dprint" (func $dprint (param i64)))
  ;; Used as `entity(entity)` and `string(string)`: two clones.
  (func $id (param i64) (result i64) (local.get 0))
  ;; Calls `$id` at whatever type it is called at itself.
  (func $id2 (param i64) (result i64) (call $id (local.get 0)))
  (func (export "use_id")
    (call $store_entity (i32.const 116) (call $id (call $load_entity (i32.const 112))))
    (call $dprint (call $id2 (call $load_string (i32.const 136)))))
  ;; One wasm local holding an entity, then a string.
  (func (export "reused_local")
    (local $p i64)
    (local.set $p (call $load_entity (i32.const 112)))
    (call $store_entity (i32.const 116) (local.get $p))
    (local.set $p (call $load_string (i32.const 136)))
    (call $dprint (local.get $p)))
  ;; Calls of function values loaded from fields.
  (func (export "call_think") (param $e i64)
    (call_indirect (type $void) (call $load_fn (local.get $e) (i32.const 44)))
    (call_indirect (type $void_float) (f32.const 1)
      (call $load_fn (local.get $e) (i32.const 45)))))

(module
  (import "env" "memory" (memory 0))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i32)))
  (import "env" "__quakec_load_fn" (func $load_fn (param i32) (result i32)))
  (import "env" "__quakec_load_entity_field_float" (func $load_float (param i32 i32) (result f32)))
  (import "env" "__quakec_store_entity_field_float" (func $store_float (param i32 i32 f32)))
  (import "env" "__quakec_store_entity_field_vector" (func $store_vector (param i32 i32 v128)))
  (import "env" "__quakec_store_entity_field_fn" (func $store_fn (param i32 i32 i32)))
  (import "env" "__quakec_load_entity_field_string" (func $load_string (param i32 i32) (result i32)))
  (import "env" "__indirect_function_table" (table $t 1 funcref))
  (import "env" "__table_base" (global $tb i32))
  (elem (global.get $tb) func $think)
  (func $think)
  ;; Float access to a vector component offset (origin_y = 11), on `self`.
  (func (export "bump_origin_y")
    (call $store_float (call $load_entity (i32.const 112)) (i32.const 11)
      (f32.add (call $load_float (call $load_entity (i32.const 112)) (i32.const 11)) (f32.const 1))))
  ;; Splat into a vector field; a named function into a function field.
  (func (export "reset") (param $e i32) (param $x f32)
    (call $store_vector (local.get $e) (i32.const 19) (f32x4.splat (local.get $x)))
    (call $store_fn (local.get $e) (i32.const 44) (i32.add (global.get $tb) (i32.const 0))))
  ;; The entity parameter's type comes from the field access; the string
  ;; result's from the field type.
  (func (export "get_classname") (param $e i32) (result i32)
    (call $load_string (local.get $e) (i32.const 28)))
  ;; Tail call of a function value.
  (func (export "call_main") (result f32)
    (return_call_indirect $t (result f32) (call $load_fn (i32.const 328)))))

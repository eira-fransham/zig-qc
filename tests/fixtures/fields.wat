(module
  (import "env" "memory" (memory 0))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result externref)))
  (import "env" "__quakec_load_fn" (func $load_fn (param i32) (result funcref)))
  (import "env" "__quakec_load_entity_field_float" (func $load_float (param externref i32) (result f32)))
  (import "env" "__quakec_store_entity_field_float" (func $store_float (param externref i32 f32)))
  (import "env" "__quakec_store_entity_field_vector" (func $store_vector (param externref i32 v128)))
  (import "env" "__quakec_store_entity_field_fn" (func $store_fn (param externref i32 funcref)))
  (import "env" "__quakec_load_entity_field_string" (func $load_string (param externref i32) (result externref)))
  (table $__funcref_call_table 1 funcref)
  (export "__funcref_call_table" (table $__funcref_call_table))
  (elem declare func $think)
  (func $think)
  ;; Float access to a vector component offset (origin_y = 11), on `self`.
  (func (export "bump_origin_y")
    (call $store_float (call $load_entity (i32.const 112)) (i32.const 11)
      (f32.add (call $load_float (call $load_entity (i32.const 112)) (i32.const 11)) (f32.const 1))))
  ;; Splat into a vector field; a named function into a function field.
  (func (export "reset") (param $e externref) (param $x f32)
    (call $store_vector (local.get $e) (i32.const 19) (f32x4.splat (local.get $x)))
    (call $store_fn (local.get $e) (i32.const 44) (ref.func $think)))
  ;; The entity parameter's type comes from the field access; the string
  ;; result's from the field type.
  (func (export "get_classname") (param $e externref) (result externref)
    (call $load_string (local.get $e) (i32.const 28)))
  ;; Tail call through the call table.
  (func (export "call_main") (result f32)
    (table.set $__funcref_call_table (i32.const 0) (call $load_fn (i32.const 328)))
    (return_call_indirect $__funcref_call_table (result f32) (i32.const 0))))

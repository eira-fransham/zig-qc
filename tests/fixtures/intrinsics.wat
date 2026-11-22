(module
  (import "env" "memory" (memory 0))
  (import "env" "__quakec_load_string" (func $load_string (param i32) (result i64)))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i64)))
  (import "env" "__quakec_store_entity" (func $store_entity (param i32 i64)))
  (import "env" "__quakec_store_fn" (func $store_fn (param i32 funcref)))
  (import "env" "__quakec_eq_string" (func $eq_string (param i64 i64) (result i32)))
  (import "env" "__quakec_not_entity" (func $not_entity (param i64) (result i32)))
  (elem declare func $think)
  (func $think)
  ;; is_map(name): mapname == name
  (func (export "is_map") (param $name i64) (result i32)
    (call $eq_string (call $load_string (i32.const 136)) (local.get $name)))
  ;; set_other(e): if e is world-ish, other = self; else other = e
  (func (export "set_other") (param $e i64)
    (if (call $not_entity (local.get $e))
      (then (call $store_entity (i32.const 116) (call $load_entity (i32.const 112))))
      (else (call $store_entity (i32.const 116) (local.get $e))))
    (call $store_fn (i32.const 332) (ref.func $think)))
  ;; time read before a store must not observe the store
  (func (export "swap_time") (param $t f32) (result f32)
    (f32.load (i32.const 124))
    (f32.store (i32.const 124) (local.get $t))))

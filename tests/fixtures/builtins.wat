(module
  (import "env" "memory" (memory 0))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i32)))
  ;; Known vanilla builtins: numbered, and their handle (i32) types are fixed.
  (import "builtin" "setmodel" (func $setmodel (param i32 i32)))
  (import "builtin" "precache_model" (func $precache_model (param i32) (result i32)))
  (import "builtin" "spawn" (func $spawn (result i32)))
  (import "builtin" "ftos" (func $ftos (param f32) (result i32)))
  (import "builtin" "dprint" (func $dprint (param i32)))
  ;; Unknown builtin: left as a prototype for the defs to define.
  (import "builtin" "my_engine_builtin" (func $custom (param f32) (result f32)))
  ;; spawn_with_model(m): e = spawn(); setmodel(e, precache_model(m))
  ;; `m` is only ever passed to builtins, so it must be inferred as a string.
  (func (export "spawn_with_model") (param $m i32) (result i32)
    (local $e i32)
    (local.set $e (call $spawn))
    (call $setmodel (local.get $e) (call $precache_model (local.get $m)))
    (local.get $e))
  (func (export "print_num") (param $x f32)
    (call $dprint (call $ftos (call $custom (local.get $x))))))

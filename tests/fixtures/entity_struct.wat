;; Entity fields accessed as struct fields: an entity is a pointer to its
;; fields, each at byte offset `4 * slot` (vanilla defs: `origin` 10,
;; `velocity` 16, `think` 44, `health` 48, `frags` 49, `netname` 74, `enemy`
;; 75). Loads and stores through an entity become field accesses.
(module
  (import "env" "memory" (memory 1))
  (import "env" "__memory_base" (global $mb i32))
  (import "env" "__table_base" (global $tb i32))
  (import "env" "__indirect_function_table" (table 1 funcref))
  (import "env" "__quakec_load_entity" (func $load_entity (param i32) (result i32)))
  (elem (global.get $tb) func $noop)
  ;; A table of 3 floats in static data: 1, 2, 4.
  (data (global.get $mb) "\00\00\80\3f\00\00\00\40\00\00\80\40")
  (func $noop)

  ;; `e.health -= d`, at a `memarg` offset. `e` is only used as an address,
  ;; so it is an entity.
  (func (export "hurt") (param $e i32) (param $d f32) (result f32)
    (f32.store offset=192 (local.get $e)
      (f32.sub (f32.load offset=192 (local.get $e)) (local.get $d)))
    (f32.load offset=192 (local.get $e)))

  ;; Through an explicit `e + k`: `self.frags`.
  (func (export "self_frags") (result f32)
    (f32.load (i32.add (call $load_entity (i32.const 112)) (i32.const 196))))

  ;; A whole vector field, read as lanes (as LLVM does `[3]f32`).
  (func (export "get_velocity") (param $e i32) (result v128)
    (v128.load32_lane 2
      (i32.add (local.get $e) (i32.const 72))
      (v128.load64_zero offset=64 (local.get $e))))

  ;; A vector field written component by component.
  (func (export "move_to") (param $e i32) (param $v v128)
    (v128.store64_lane offset=40 0 (local.get $e) (local.get $v))
    (f32.store offset=48 (local.get $e) (f32x4.extract_lane 2 (local.get $v))))

  ;; String, entity and function fields.
  (func (export "rename") (param $e i32) (param $s i32)
    (i32.store offset=296 (local.get $e) (local.get $s)))
  (func (export "enemy_health") (param $e i32) (result f32)
    (f32.load offset=192 (i32.load offset=300 (local.get $e))))
  (func (export "set_think") (param $e i32)
    (i32.store offset=176 (local.get $e) (global.get $tb)))

  ;; A table in static data, indexed by a variable: memory, not a field.
  (func (export "table") (param $i i32) (result f32)
    (f32.load (i32.add (global.get $mb) (i32.shl (local.get $i) (i32.const 2))))))

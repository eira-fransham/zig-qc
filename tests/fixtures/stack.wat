;; Locals in the shadow stack, as LLVM keeps them (e.g. volatile locals):
;; a frame below `__stack_pointer`, allocated on entry and freed on return.
;; Each slot becomes an SSA value.
(module
  (import "env" "memory" (memory 1))
  (import "env" "__stack_pointer" (global $sp (mut i32)))
  (import "env" "__quakec_load_entity_field_float" (func $load_float (param i64 i32) (result f32)))

  ;; Two slots, at `memarg` offsets from the frame.
  (func (export "sum") (param $a f32) (param $b f32) (result f32)
    (local $fp i32)
    (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 16))))
    (f32.store offset=8 (local.get $fp) (local.get $a))
    (f32.store offset=12 (local.get $fp) (local.get $b))
    (f32.store offset=8 (local.get $fp)
      (f32.add (f32.load offset=8 (local.get $fp)) (f32.const 1)))
    (global.set $sp (i32.add (local.get $fp) (i32.const 16)))
    (f32.add (f32.load offset=8 (local.get $fp)) (f32.load offset=12 (local.get $fp))))

  ;; A slot read before it is written (uninitialized: 0), and a constant
  ;; stored as `i32` bits into an `f32` slot (as LLVM does).
  (func (export "uninit") (result f32)
    (local $fp i32) (local $r f32)
    (local.set $fp (i32.sub (global.get $sp) (i32.const 16)))
    (local.set $r (f32.load offset=4 (local.get $fp)))
    (i32.store offset=4 (local.get $fp) (i32.const 0x40400000)) ;; 3.0
    (f32.add (local.get $r) (f32.load offset=4 (local.get $fp))))

  ;; A counter in a slot, around a loop.
  (func (export "count") (param $n f32) (result f32)
    (local $fp i32)
    (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 16))))
    (i32.store offset=12 (local.get $fp) (i32.const 0))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (f32.lt (f32.load offset=12 (local.get $fp)) (local.get $n))))
        (f32.store offset=12 (local.get $fp)
          (f32.add (f32.load offset=12 (local.get $fp)) (f32.const 1)))
        (br $l)))
    (global.set $sp (i32.add (local.get $fp) (i32.const 16)))
    (f32.load offset=12 (local.get $fp)))

  ;; A load through a pointer to one of two slots.
  (func (export "choose") (param $c f32) (param $a f32) (param $b f32) (result f32)
    (local $fp i32)
    (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 16))))
    (f32.store offset=8 (local.get $fp) (local.get $a))
    (f32.store offset=12 (local.get $fp) (local.get $b))
    (global.set $sp (i32.add (local.get $fp) (i32.const 16)))
    (f32.load
      (select
        (i32.add (local.get $fp) (i32.const 8))
        (i32.add (local.get $fp) (i32.const 12))
        (f32.ne (local.get $c) (f32.const 0)))))

  ;; An entity in an `i64` slot (field 46 is `nextthink`).
  (func (export "read_nextthink") (param $e i64) (result f32)
    (local $fp i32)
    (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 16))))
    (i64.store offset=8 (local.get $fp) (local.get $e))
    (call $load_float (i64.load offset=8 (local.get $fp)) (i32.const 46))
    (global.set $sp (i32.add (local.get $fp) (i32.const 16))))

  ;; A frame that is never used.
  (func (export "empty_frame") (param $x f32) (result f32)
    (local $fp i32)
    (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 32))))
    (global.set $sp (i32.add (local.get $fp) (i32.const 32)))
    (f32.mul (local.get $x) (f32.const 2))))

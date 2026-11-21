(module
  (import "env" "memory" (memory 1))
  (import "env" "__memory_base" (global $base i32))
  ;; f32s 1.5 and 2.5 at +0, then a u8 table of 10 entries at +8
  ;; (two of them zero, which the accessor skips).
  (data (global.get $base) "\00\00\c0\3f\00\00\20\40")
  (data (offset (i32.add (global.get $base) (i32.const 8)))
    "\01\02\00\04\05\06\07\00\09\ff")
  ;; A constant address: folded to a literal (2.5).
  (func (export "second") (result f32)
    (f32.load offset=4 (global.get $base)))
  ;; A dynamic index into the u8 table: a binary-search accessor.
  (func (export "table") (param $i i32) (result f32)
    (f32.convert_i32_u
      (i32.load8_u offset=8 (i32.add (global.get $base) (local.get $i))))))

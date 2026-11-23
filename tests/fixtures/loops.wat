(module
  (import "env" "memory" (memory 0))
  (global $g (mut f32) (f32.const 0))
  ;; Nested loops. The inner loop continues itself (j == 2), and continues
  ;; (i == 5, j == 3) and breaks (i * j > 30) the outer loop.
  (func (export "nested") (param $n i32) (result i32)
    (local $i i32) (local $j i32) (local $c i32)
    (block $outer_done
      (loop $outer
        (br_if $outer_done (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $j (i32.const 0))
        (block $inner_done
          (loop $inner
            (br_if $inner_done (i32.ge_s (local.get $j) (local.get $i)))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))
            (br_if $inner (i32.eq (local.get $j) (i32.const 2)))
            (if (i32.and (i32.eq (local.get $i) (i32.const 5))
                         (i32.eq (local.get $j) (i32.const 3)))
              (then
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br $outer)))
            (br_if $outer_done (i32.gt_s (i32.mul (local.get $i) (local.get $j))
                                         (i32.const 30)))
            (local.set $c (i32.add (local.get $c) (i32.const 1)))
            (br $inner)))
        (local.set $c (i32.add (local.get $c) (i32.const 100)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $outer)))
    (local.get $c))
  ;; A loop tested at the end: the sum of the squares below `n` (at least
  ;; one iteration).
  (func (export "do_while") (param $n i32) (result i32)
    (local $i i32) (local $acc i32)
    (loop $top
      (local.set $acc (i32.add (local.get $acc) (i32.mul (local.get $i) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $top (i32.lt_s (local.get $i) (local.get $n))))
    (local.get $acc))
  ;; A loop whose header has a side effect before its test.
  (func (export "header_effects") (param $n f32) (result f32)
    (local $k f32)
    (block $done
      (loop $top
        (global.set $g (f32.add (global.get $g) (f32.const 1)))
        (br_if $done (f32.ge (global.get $g) (local.get $n)))
        (local.set $k (f32.add (local.get $k) (f32.const 2)))
        (br $top)))
    (f32.add (local.get $k) (global.get $g)))
  ;; A `br_table` in a loop: its arms continue the loop, return, and break
  ;; out of it (which needs a `goto` from inside a `switch`).
  (func (export "switch_loop") (param $n i32) (result i32)
    (local $i i32) (local $acc i32)
    (block $done
      (loop $top
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br_if $done (i32.gt_s (local.get $i) (local.get $n)))
        (block $c3
          (block $c2
            (block $c1
              (block $c0
                (br_table $c0 $c1 $c2 $c3 (i32.rem_u (local.get $i) (i32.const 4))))
              (br $top))
            (local.set $acc (i32.add (local.get $acc) (local.get $i)))
            (br $top))
          (if (i32.gt_s (local.get $acc) (i32.const 50))
            (then (return (i32.mul (local.get $acc) (i32.const 1000)))))
          (br $top))
        (br_if $done (i32.gt_s (local.get $i) (i32.const 9)))
        (local.set $acc (i32.add (local.get $acc) (i32.const 100)))
        (br $top)))
    (local.get $acc))
  ;; A loop left only by returning (with a side effect in its header, so
  ;; it's a `while (1)`): the square root of `n` if it's a square, else -1.
  (func (export "find_root") (param $n i32) (result i32)
    (local $i i32)
    (loop $top
      (global.set $g (f32.add (global.get $g) (f32.const 1)))
      (if (i32.eq (i32.mul (local.get $i) (local.get $i)) (local.get $n))
        (then (return (local.get $i))))
      (if (i32.gt_s (i32.mul (local.get $i) (local.get $i)) (local.get $n))
        (then (return (i32.const -1))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $top))
    (unreachable)))

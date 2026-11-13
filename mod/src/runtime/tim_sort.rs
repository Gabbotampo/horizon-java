//! `java.util.TimSort` (and `ComparableTimSort`, the same algorithm), as
//! JDK 25 runs it: the sort behind `List.sort`, `Collections.sort`,
//! `Arrays.sort(Object[])` and `Stream.sorted`.
//!
//! The result of a stable sort is fixed, but the comparisons that reach it
//! are not, and a jar's `compareTo` or comparator can count, log or throw.
//! This port makes the same `compare` calls in the same order as the JDK:
//! a run of fewer than 32 elements is extended and binary-insertion sorted;
//! longer inputs are cut into natural runs (at least `minRunLength` long),
//! kept on a stack under TimSort's invariants and merged with galloping
//! (`MIN_GALLOP` 7, adapted per merge). A comparator that breaks its
//! contract can make a merge throw `IllegalArgumentException("Comparison
//! method violates its general contract!")`, as the JDK's does.
//!
//! The array is sorted in place, and a `compare` that throws leaves it as
//! the JDK leaves it at that point (a merge in progress keeps part of one run
//! in its temporary array): `ArrayList.sort` writes that state back.

use crate::runtime::{NativeError, Value};

/// `TimSort.MIN_MERGE`.
const MIN_MERGE: usize = 32;
/// `TimSort.MIN_GALLOP`.
const MIN_GALLOP: usize = 7;

/// `c.compare(a, b)`.
pub(crate) type Compare<'c> = dyn FnMut(&Value, &Value) -> Result<i32, NativeError> + 'c;

fn contract_violation() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some("Comparison method violates its general contract!".into()),
        frame: None,
    }
}

/// `TimSort.sort(a, 0, a.length, c, null, 0, 0)`.
pub(crate) fn sort(a: &mut [Value], compare: &mut Compare<'_>) -> Result<(), NativeError> {
    let hi = a.len();
    let mut lo = 0;
    let mut remaining = hi;
    if remaining < 2 {
        return Ok(());
    }
    if remaining < MIN_MERGE {
        let run = count_run_and_make_ascending(a, lo, hi, compare)?;
        return binary_sort(a, lo, hi, lo + run, compare);
    }
    let mut sorter = Sorter {
        a,
        compare,
        min_gallop: MIN_GALLOP,
        runs: Vec::new(),
    };
    let min_run = min_run_length(remaining);
    while remaining != 0 {
        let mut run = count_run_and_make_ascending(sorter.a, lo, hi, sorter.compare)?;
        if run < min_run {
            let force = remaining.min(min_run);
            binary_sort(sorter.a, lo, lo + force, lo + run, sorter.compare)?;
            run = force;
        }
        sorter.runs.push((lo, run));
        sorter.merge_collapse()?;
        lo += run;
        remaining -= run;
    }
    sorter.merge_force_collapse()
}

/// `binarySort`: insert `a[start..hi]` into the sorted `a[lo..start]`.
fn binary_sort(
    a: &mut [Value],
    lo: usize,
    hi: usize,
    start: usize,
    compare: &mut Compare<'_>,
) -> Result<(), NativeError> {
    let mut start = if start == lo { start + 1 } else { start };
    while start < hi {
        let pivot = a[start].clone();
        let (mut left, mut right) = (lo, start);
        while left < right {
            let mid = (left + right) >> 1;
            if compare(&pivot, &a[mid])? < 0 {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        a[left..=start].rotate_right(1);
        a[left] = pivot;
        start += 1;
    }
    Ok(())
}

/// `countRunAndMakeAscending`: the length of the run at `lo`, reversed in
/// place when it is strictly descending.
fn count_run_and_make_ascending(
    a: &mut [Value],
    lo: usize,
    hi: usize,
    compare: &mut Compare<'_>,
) -> Result<usize, NativeError> {
    let mut run_hi = lo + 1;
    if run_hi == hi {
        return Ok(1);
    }
    run_hi += 1;
    if compare(&a[run_hi - 1], &a[lo])? < 0 {
        while run_hi < hi && compare(&a[run_hi], &a[run_hi - 1])? < 0 {
            run_hi += 1;
        }
        a[lo..run_hi].reverse();
    } else {
        while run_hi < hi && compare(&a[run_hi], &a[run_hi - 1])? >= 0 {
            run_hi += 1;
        }
    }
    Ok(run_hi - lo)
}

/// `minRunLength`.
fn min_run_length(mut n: usize) -> usize {
    let mut r = 0;
    while n >= MIN_MERGE {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// `gallopLeft`: the leftmost position in the sorted `run` (`base`, `len`
/// into `values`) where `key` belongs, searching from `hint`.
fn gallop_left(
    key: &Value,
    values: &[Value],
    base: usize,
    len: usize,
    hint: usize,
    compare: &mut Compare<'_>,
) -> Result<usize, NativeError> {
    let (mut last_ofs, mut ofs): (isize, isize) = (0, 1);
    let hint = hint as isize;
    let at = |i: isize| &values[(base as isize + i) as usize];
    if compare(key, at(hint))? > 0 {
        let max_ofs = len as isize - hint;
        while ofs < max_ofs && compare(key, at(hint + ofs))? > 0 {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        last_ofs += hint;
        ofs += hint;
    } else {
        let max_ofs = hint + 1;
        while ofs < max_ofs && compare(key, at(hint - ofs))? <= 0 {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        let tmp = last_ofs;
        last_ofs = hint - ofs;
        ofs = hint - tmp;
    }
    last_ofs += 1;
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) >> 1);
        if compare(key, at(m))? > 0 {
            last_ofs = m + 1;
        } else {
            ofs = m;
        }
    }
    Ok(ofs as usize)
}

/// `gallopRight`: like [`gallop_left`], but the rightmost position.
fn gallop_right(
    key: &Value,
    values: &[Value],
    base: usize,
    len: usize,
    hint: usize,
    compare: &mut Compare<'_>,
) -> Result<usize, NativeError> {
    let (mut last_ofs, mut ofs): (isize, isize) = (0, 1);
    let hint = hint as isize;
    let at = |i: isize| &values[(base as isize + i) as usize];
    if compare(key, at(hint))? < 0 {
        let max_ofs = hint + 1;
        while ofs < max_ofs && compare(key, at(hint - ofs))? < 0 {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        let tmp = last_ofs;
        last_ofs = hint - ofs;
        ofs = hint - tmp;
    } else {
        let max_ofs = len as isize - hint;
        while ofs < max_ofs && compare(key, at(hint + ofs))? >= 0 {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        last_ofs += hint;
        ofs += hint;
    }
    last_ofs += 1;
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) >> 1);
        if compare(key, at(m))? < 0 {
            ofs = m;
        } else {
            last_ofs = m + 1;
        }
    }
    Ok(ofs as usize)
}

/// One sort's state: the array, the comparator, the adaptive gallop
/// threshold and the stack of pending runs (`runBase`, `runLen`).
struct Sorter<'a, 'c> {
    a: &'a mut [Value],
    compare: &'a mut Compare<'c>,
    min_gallop: usize,
    runs: Vec<(usize, usize)>,
}

impl Sorter<'_, '_> {
    /// `mergeCollapse`: merge until the run stack's invariants hold.
    fn merge_collapse(&mut self) -> Result<(), NativeError> {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            let len = |i: usize| self.runs[i].1;
            if (n > 0 && len(n - 1) <= len(n) + len(n + 1))
                || (n > 1 && len(n - 2) <= len(n) + len(n - 1))
            {
                if len(n - 1) < len(n + 1) {
                    n -= 1;
                }
            } else if len(n) > len(n + 1) {
                break;
            }
            self.merge_at(n)?;
        }
        Ok(())
    }

    /// `mergeForceCollapse`: merge everything left.
    fn merge_force_collapse(&mut self) -> Result<(), NativeError> {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if n > 0 && self.runs[n - 1].1 < self.runs[n + 1].1 {
                n -= 1;
            }
            self.merge_at(n)?;
        }
        Ok(())
    }

    /// `mergeAt(i)`: merge runs `i` and `i + 1`.
    fn merge_at(&mut self, i: usize) -> Result<(), NativeError> {
        let (mut base1, mut len1) = self.runs[i];
        let (base2, mut len2) = self.runs[i + 1];
        self.runs[i].1 = len1 + len2;
        self.runs.remove(i + 1);
        let key = self.a[base2].clone();
        let k = gallop_right(&key, self.a, base1, len1, 0, self.compare)?;
        base1 += k;
        len1 -= k;
        if len1 == 0 {
            return Ok(());
        }
        let key = self.a[base1 + len1 - 1].clone();
        len2 = gallop_left(&key, self.a, base2, len2, len2 - 1, self.compare)?;
        if len2 == 0 {
            return Ok(());
        }
        if len1 <= len2 {
            self.merge_lo(base1, len1, base2, len2)
        } else {
            self.merge_hi(base1, len1, base2, len2)
        }
    }

    /// `mergeLo`: the first run (the shorter) goes to a temporary array.
    fn merge_lo(
        &mut self,
        base1: usize,
        mut len1: usize,
        base2: usize,
        mut len2: usize,
    ) -> Result<(), NativeError> {
        let tmp: Vec<Value> = self.a[base1..base1 + len1].to_vec();
        let (mut cursor1, mut cursor2, mut dest) = (0usize, base2, base1);
        self.a[dest] = self.a[cursor2].clone();
        dest += 1;
        cursor2 += 1;
        len2 -= 1;
        if len2 == 0 {
            self.a[dest..dest + len1].clone_from_slice(&tmp[cursor1..cursor1 + len1]);
            return Ok(());
        }
        if len1 == 1 {
            for offset in 0..len2 {
                self.a[dest + offset] = self.a[cursor2 + offset].clone();
            }
            self.a[dest + len2] = tmp[cursor1].clone();
            return Ok(());
        }
        let mut min_gallop = self.min_gallop;
        'outer: loop {
            let (mut count1, mut count2) = (0usize, 0usize);
            loop {
                if (self.compare)(&self.a[cursor2], &tmp[cursor1])? < 0 {
                    self.a[dest] = self.a[cursor2].clone();
                    dest += 1;
                    cursor2 += 1;
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 0 {
                        break 'outer;
                    }
                } else {
                    self.a[dest] = tmp[cursor1].clone();
                    dest += 1;
                    cursor1 += 1;
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }
            loop {
                let key = self.a[cursor2].clone();
                count1 = gallop_right(&key, &tmp, cursor1, len1, 0, self.compare)?;
                if count1 != 0 {
                    self.a[dest..dest + count1].clone_from_slice(&tmp[cursor1..cursor1 + count1]);
                    dest += count1;
                    cursor1 += count1;
                    len1 -= count1;
                    if len1 <= 1 {
                        break 'outer;
                    }
                }
                self.a[dest] = self.a[cursor2].clone();
                dest += 1;
                cursor2 += 1;
                len2 -= 1;
                if len2 == 0 {
                    break 'outer;
                }
                let key = tmp[cursor1].clone();
                count2 = gallop_left(&key, self.a, cursor2, len2, 0, self.compare)?;
                if count2 != 0 {
                    for offset in 0..count2 {
                        self.a[dest + offset] = self.a[cursor2 + offset].clone();
                    }
                    dest += count2;
                    cursor2 += count2;
                    len2 -= count2;
                    if len2 == 0 {
                        break 'outer;
                    }
                }
                self.a[dest] = tmp[cursor1].clone();
                dest += 1;
                cursor1 += 1;
                len1 -= 1;
                if len1 == 1 {
                    break 'outer;
                }
                min_gallop = min_gallop.saturating_sub(1);
                if !(count1 >= MIN_GALLOP || count2 >= MIN_GALLOP) {
                    break;
                }
            }
            min_gallop += 2;
        }
        self.min_gallop = min_gallop.max(1);
        if len1 == 1 {
            for offset in 0..len2 {
                self.a[dest + offset] = self.a[cursor2 + offset].clone();
            }
            self.a[dest + len2] = tmp[cursor1].clone();
        } else if len1 == 0 {
            return Err(contract_violation());
        } else {
            self.a[dest..dest + len1].clone_from_slice(&tmp[cursor1..cursor1 + len1]);
        }
        Ok(())
    }

    /// `mergeHi`: the second run (the shorter) goes to a temporary array,
    /// merged from the right.
    fn merge_hi(
        &mut self,
        base1: usize,
        mut len1: usize,
        base2: usize,
        mut len2: usize,
    ) -> Result<(), NativeError> {
        let tmp: Vec<Value> = self.a[base2..base2 + len2].to_vec();
        let mut cursor1 = (base1 + len1) as isize - 1;
        let mut cursor2 = len2 as isize - 1;
        let mut dest = (base2 + len2) as isize - 1;
        let a = |i: isize| i as usize;
        self.a[a(dest)] = self.a[a(cursor1)].clone();
        dest -= 1;
        cursor1 -= 1;
        len1 -= 1;
        if len1 == 0 {
            let start = a(dest - (len2 as isize - 1));
            self.a[start..start + len2].clone_from_slice(&tmp[..len2]);
            return Ok(());
        }
        if len2 == 1 {
            dest -= len1 as isize;
            cursor1 -= len1 as isize;
            for offset in (0..len1).rev() {
                self.a[a(dest + 1) + offset] = self.a[a(cursor1 + 1) + offset].clone();
            }
            self.a[a(dest)] = tmp[a(cursor2)].clone();
            return Ok(());
        }
        let mut min_gallop = self.min_gallop;
        'outer: loop {
            let (mut count1, mut count2) = (0usize, 0usize);
            loop {
                if (self.compare)(&tmp[a(cursor2)], &self.a[a(cursor1)])? < 0 {
                    self.a[a(dest)] = self.a[a(cursor1)].clone();
                    dest -= 1;
                    cursor1 -= 1;
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 0 {
                        break 'outer;
                    }
                } else {
                    self.a[a(dest)] = tmp[a(cursor2)].clone();
                    dest -= 1;
                    cursor2 -= 1;
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }
            loop {
                let key = tmp[a(cursor2)].clone();
                count1 = len1 - gallop_right(&key, self.a, base1, len1, len1 - 1, self.compare)?;
                if count1 != 0 {
                    dest -= count1 as isize;
                    cursor1 -= count1 as isize;
                    len1 -= count1;
                    for offset in (0..count1).rev() {
                        self.a[a(dest + 1) + offset] = self.a[a(cursor1 + 1) + offset].clone();
                    }
                    if len1 == 0 {
                        break 'outer;
                    }
                }
                self.a[a(dest)] = tmp[a(cursor2)].clone();
                dest -= 1;
                cursor2 -= 1;
                len2 -= 1;
                if len2 == 1 {
                    break 'outer;
                }
                let key = self.a[a(cursor1)].clone();
                count2 = len2 - gallop_left(&key, &tmp, 0, len2, len2 - 1, self.compare)?;
                if count2 != 0 {
                    dest -= count2 as isize;
                    cursor2 -= count2 as isize;
                    len2 -= count2;
                    let (from, to) = (a(cursor2 + 1), a(dest + 1));
                    self.a[to..to + count2].clone_from_slice(&tmp[from..from + count2]);
                    if len2 <= 1 {
                        break 'outer;
                    }
                }
                self.a[a(dest)] = self.a[a(cursor1)].clone();
                dest -= 1;
                cursor1 -= 1;
                len1 -= 1;
                if len1 == 0 {
                    break 'outer;
                }
                min_gallop = min_gallop.saturating_sub(1);
                if !(count1 >= MIN_GALLOP || count2 >= MIN_GALLOP) {
                    break;
                }
            }
            min_gallop += 2;
        }
        self.min_gallop = min_gallop.max(1);
        if len2 == 1 {
            dest -= len1 as isize;
            cursor1 -= len1 as isize;
            for offset in (0..len1).rev() {
                self.a[a(dest + 1) + offset] = self.a[a(cursor1 + 1) + offset].clone();
            }
            self.a[a(dest)] = tmp[a(cursor2)].clone();
        } else if len2 == 0 {
            return Err(contract_violation());
        } else {
            let start = a(dest - (len2 as isize - 1));
            self.a[start..start + len2].clone_from_slice(&tmp[..len2]);
        }
        Ok(())
    }
}

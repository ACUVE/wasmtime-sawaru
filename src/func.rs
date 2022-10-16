pub(super) fn gcd(mut a: i32, mut b: i32) -> i32 {
    loop {
        if a == 0 {
            return b;
        }
        b %= a;
        if b == 0 {
            return a;
        }
        a %= b;
    }
}

pub(super) fn is_prime(target: u64) -> Option<bool> {
    let mut i: u64 = 2;

    loop {
        match i.checked_mul(i) {
            Some(x) if x > target => return Some(true),
            Some(_) if target % i == 0 => return Some(false),
            Some(_) => i += 1,
            None => return None,
        }
    }
}

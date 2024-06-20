use num_bigint::BigInt;

// fast way to remove decimals from amounts and compare to find arb (NOT IDEAL)
pub fn make_same_digits(mut big_int1: BigInt, mut big_int2: BigInt) -> (BigInt, BigInt) {
    loop {
        let num_digits1 = big_int1.to_string().len();
        let num_digits2 = big_int2.to_string().len();

        if num_digits1 > num_digits2 {
            big_int1 = big_int1 / 10;
        } else if num_digits1 < num_digits2 {
            big_int2 = big_int2 / 10;
        } else {
            break;
        }
    }

    (big_int1, big_int2)
}

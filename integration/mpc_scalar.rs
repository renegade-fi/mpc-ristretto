use curve25519_dalek::scalar::Scalar;

use mpc_ristretto::{
    beaver::SharedValueSource,
    error::MpcNetworkError,
    mpc_scalar::{scalar_to_u64, MpcScalar},
    network::QuicTwoPartyNet,
};
use rand::{thread_rng, RngCore};

use crate::{IntegrationTest, IntegrationTestArgs};

/// Returns beaver triples (0, 0, 0) for party 0 and (1, 1, 1) for party 1
#[derive(Debug)]
pub(crate) struct PartyIDBeaverSource {
    party_id: u64,
}

impl PartyIDBeaverSource {
    pub fn new(party_id: u64) -> Self {
        Self { party_id }
    }
}

/// The PartyIDBeaverSource returns beaver triplets split statically between the
/// parties. We assume a = 2, b = 3 ==> c = 6. [a] = (1, 1); [b] = (3, 0) [c] = (2, 4)
impl SharedValueSource<Scalar> for PartyIDBeaverSource {
    fn next_shared_bit(&mut self) -> Scalar {
        // Simply output partyID, assume partyID \in {0, 1}
        assert!(self.party_id == 0 || self.party_id == 1);
        Scalar::from(self.party_id as u64)
    }

    fn next_triplet(&mut self) -> (Scalar, Scalar, Scalar) {
        if self.party_id == 0 {
            (Scalar::from(1u64), Scalar::from(3u64), Scalar::from(2u64))
        } else {
            (Scalar::from(1u64), Scalar::from(0u64), Scalar::from(4u64))
        }
    }

    fn next_shared_inverse_pair(&mut self) -> (Scalar, Scalar) {
        (Scalar::one(), Scalar::one())
    }

    fn next_shared_value(&mut self) -> Scalar {
        Scalar::from(self.party_id)
    }
}

/// Simple test of an add circuit with different visibilities
fn test_add(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Party 0 holds 42 and party 1 holds 33
    let value = if test_args.party_id == 0 { 42 } else { 33 };

    let my_value = MpcScalar::from_private_u64(
        value,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    let value1_shared = my_value
        .share_secret(0 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let value2_shared = my_value
        .share_secret(1 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let public_value = MpcScalar::from_public_u64(
        58,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Shared value + shared value
    let double_shared = (&value1_shared + value2_shared)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(75u64);

    if double_shared.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&double_shared.value())
        ));
    }

    // Shared value + public value
    let shared_public = (&value1_shared + &public_value)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(100u64);

    if shared_public.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&shared_public.value())
        ));
    }

    // Public value + public value
    let public_public = (&public_value + &public_value)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(116u64);

    if public_public.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&public_public.value())
        ));
    }

    Ok(())
}

fn test_sub(test_args: &IntegrationTestArgs) -> Result<(), String> {
    let value = if test_args.party_id == 0 { 10 } else { 6 };
    let my_value = MpcScalar::from_private_u64(
        value,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Share values with counterparty
    let shared_value1 = my_value
        .share_secret(0 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let shared_value2 = my_value
        .share_secret(1 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let public_value = MpcScalar::from_public_u64(
        15u64,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Shared value - shared value
    let shared_shared = (&shared_value1 - &shared_value2)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(4u8);

    if shared_shared.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&shared_shared.value())
        ));
    }

    // Public value - shared value
    let public_shared = (&public_value - &shared_value1)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(5u8);

    if public_shared.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&public_shared.value())
        ));
    }

    // Public value - public value
    #[allow(clippy::eq_op)]
    let public_public = (&public_value - &public_value)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(0u8);

    if public_public.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&public_public.value())
        ));
    }

    Ok(())
}

/// Tests multiplication with different visibilities
fn test_mul(test_args: &IntegrationTestArgs) -> Result<(), String> {
    let value = if test_args.party_id == 0 { 10 } else { 6 };
    let my_value = MpcScalar::from_private_u64(
        value,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Share values with counterparty
    let shared_value1 = my_value
        .share_secret(0 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let shared_value2 = my_value
        .share_secret(1 /* party_id */)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let public_value = MpcScalar::from_public_u64(
        15u64,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Shared value * shared value
    let shared_shared = (&shared_value1 * &shared_value2)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(60u64);

    if shared_shared.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&shared_shared.value())
        ));
    }

    // Public value * shared value
    let public_shared = (&public_value * &shared_value1)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(150u64);

    if public_shared.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&public_shared.value())
        ));
    }

    // Public value * public value
    let public_public = (&public_value * &public_value)
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;
    let expected = Scalar::from(225u64);

    if public_public.value().ne(&expected) {
        return Err(format!(
            "Expected {}, got {}",
            scalar_to_u64(&expected),
            scalar_to_u64(&public_public.value())
        ));
    }

    Ok(())
}

/// Tests batch multiplication
fn test_batch_mul(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Intersperse private and public values
    let values = (0..10)
        .map(|val| {
            if val % 2 == 0 {
                MpcScalar::from_public_u64(
                    val,
                    test_args.net_ref.clone(),
                    test_args.beaver_source.clone(),
                )
            } else {
                let val = MpcScalar::from_private_u64(
                    val as u64,
                    test_args.net_ref.clone(),
                    test_args.beaver_source.clone(),
                );

                val.share_secret(0 /* party_id */).unwrap()
            }
        })
        .collect::<Vec<_>>();

    // Multiply the values array with itself
    let res = MpcScalar::batch_mul(&values, &values)
        .map_err(|err| format!("Error performing batch_mul: {:?}", err))?;

    // Convert to u64 for comparison
    let res_u64 = res
        .iter()
        .map(|val| scalar_to_u64(&val.open().unwrap().to_scalar()))
        .collect::<Vec<_>>();

    let expected = (0..10).map(|x| (x * x) as u64).collect::<Vec<_>>();
    if expected.ne(&res_u64) {
        return Err(format!("Expected: {:?}, got {:?}", expected, res_u64));
    }

    Ok(())
}

/// Party 0 shares a value then opens it, the result should be the initial value
fn test_open_value(test_args: &IntegrationTestArgs) -> Result<(), String> {
    let val: u64 = 42;
    let private_val = MpcScalar::from_private_u64(
        val,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    let share = private_val
        .share_secret(0)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;

    let opened_val = share
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;

    if MpcScalar::from_public_u64(
        val,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    )
    .eq(&opened_val)
    {
        Ok(())
    } else {
        Err(format!("Expected {} got {:?}", val, opened_val))
    }
}

fn test_commit_and_open(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Both parties commit and open a value
    let shared_value = MpcScalar::from_private_u64(
        42,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    )
    .share_secret(0 /* party_id */) // Only party 0 shares
    .map_err(|err| format!("Error sharing value: {:?}", err))?;

    let res = shared_value
        .commit_and_open()
        .map_err(|err| format!("Error commiting and opening value: {:?}", err))?;

    if res.value().ne(&Scalar::from(42u64)) {
        return Err(format!(
            "Expected {}, got {}",
            42,
            scalar_to_u64(&res.value())
        ));
    }

    Ok(())
}

/// Test that sharing a batch of values works properly
fn test_open_batch(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Party 0 shares values with party 1
    let values: Vec<MpcScalar<QuicTwoPartyNet, PartyIDBeaverSource>> = vec![1u64, 2u64, 3u64]
        .into_iter()
        .map(|value| {
            MpcScalar::from_private_u64(
                value,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
        })
        .collect();

    let shared_values = MpcScalar::batch_share_secrets(0 /* party_id */, &values)
        .map_err(|err| format!("Error sharing values: {:?}", err))?;

    // Open the batch and verify equality
    let opened_values = MpcScalar::batch_open(&shared_values)
        .map_err(|err| format!("Error opening values: {:?}", err))?;

    if opened_values.ne(&values) {
        return Err(format!("Expected: {:?}, Got: {:?}", values, opened_values));
    }

    Ok(())
}

/// Tests that committing and opening in a batch works properly
fn test_commit_and_open_batch(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Party 0 shares a vector of values, both parties commit and open
    let values: Vec<MpcScalar<QuicTwoPartyNet, PartyIDBeaverSource>> = vec![1u64, 2u64, 3u64]
        .into_iter()
        .map(|value| {
            MpcScalar::from_private_u64(
                value,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
        })
        .collect();

    let shared_values = MpcScalar::batch_share_secrets(0 /* party_id */, &values)
        .map_err(|err| format!("Error sharing values: {:?}", err))?;

    // Open validly and verify that opening passes
    let opened_values = MpcScalar::batch_commit_and_open(&shared_values)
        .map_err(|err| format!("Error committing and opening values: {:?}", err))?;

    if opened_values.ne(&values) {
        return Err(format!("Expected: {:?}, Got: {:?}", values, opened_values));
    }

    Ok(())
}

/// Party 0 sends a value and party 1 receives
fn test_receive_value(test_args: &IntegrationTestArgs) -> Result<(), String> {
    let share = {
        if test_args.party_id == 0 {
            // Send 10 as an MpcScalar
            MpcScalar::from_private_u64(
                10,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
            .share_secret(0 /* party_id */)
            .map_err(|err| format!("Error sharing secret: {:?}", err))?
        } else {
            MpcScalar::receive_value(test_args.net_ref.clone(), test_args.beaver_source.clone())
                .map_err(|err| format!("Error receiving value: {:?}", err))?
        }
    };

    let share_opened = share
        .open()
        .map_err(|err| format!("Error opening share: {:?}", err))?;
    if !share_opened.value().eq(&Scalar::from(10u64)) {
        return Err(format!(
            "Expected {}, got {}",
            10,
            scalar_to_u64(&share_opened.value())
        ));
    }

    Ok(())
}

/// Tests summing over a sequence of shared values
fn test_sum(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Party 0 allocates the first values list, party 1 allocates the second list
    let values: Vec<u64> = if test_args.party_id == 0 {
        vec![1, 2, 3]
    } else {
        vec![4, 5, 6]
    };

    let network_values: Vec<MpcScalar<_, _>> = values
        .into_iter()
        .map(|value| {
            MpcScalar::from_public_u64(
                value,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
        })
        .collect();

    // Share values with peer
    let shared_values1: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|value| value.share_secret(0 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing party 0 values: {:?}", err))?;

    let shared_values2: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|value| value.share_secret(1 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing party 1 values: {:?}", err))?;

    // Sum over all values; we expect 1 + 2 + 3 + 4 + 5 + 6 = 21
    let shared_sum: MpcScalar<_, _> = shared_values1.iter().chain(shared_values2.iter()).sum();

    let res = shared_sum
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;

    let expected = MpcScalar::from_public_u64(
        21,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    if res.eq(&expected) {
        Ok(())
    } else {
        Err(format!(
            "Expected: {:?}\nGot: {:?}\n",
            expected.value(),
            res.value()
        ))
    }
}

/// Tests the product over a series of values
fn test_product(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Party 0 allocates the first values list, party 1 allocates the second list
    let values: Vec<u64> = if test_args.party_id == 0 {
        vec![1, 2, 3]
    } else {
        vec![4, 5, 6]
    };

    let network_values: Vec<MpcScalar<_, _>> = values
        .into_iter()
        .map(|value| {
            MpcScalar::from_public_u64(
                value,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
        })
        .collect();

    // Share values with peer
    let shared_values1: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|value| value.share_secret(0 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing party 0 values: {:?}", err))?;

    let shared_values2: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|value| value.share_secret(1 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing party 1 values: {:?}", err))?;

    // Take the product over all values, we expecte 1 * 2 * 3 * 4 * 5 * 6 = 720
    let shared_product: MpcScalar<_, _> =
        shared_values1.iter().chain(shared_values2.iter()).product();

    let res = shared_product
        .open()
        .map_err(|err| format!("Error opening value: {:?}", err))?;

    let expected = MpcScalar::from_public_u64(
        720,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    if res.eq(&expected) {
        Ok(())
    } else {
        Err(format!(
            "Expected: {:?}\nGot: {:?}\n",
            expected.value(),
            res.value()
        ))
    }
}

/// Tests that taking a linear combination of shared values works properly
fn test_linear_combination(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Assume that party 0 allocates the values and party 1 allocates the coefficients
    let network_values: Vec<MpcScalar<_, _>> = {
        if test_args.party_id == 0 {
            1..6
        } else {
            7..12
        }
    }
    .map(|a| {
        MpcScalar::from_public_u64(
            a,
            test_args.net_ref.clone(),
            test_args.beaver_source.clone(),
        )
    })
    .collect::<Vec<MpcScalar<_, _>>>();

    // Share the values
    let shared_values: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|val| val.share_secret(0 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing values: {:?}", err))?;

    let shared_coeffs: Vec<MpcScalar<_, _>> = network_values
        .iter()
        .map(|val| val.share_secret(1 /* party_id */))
        .collect::<Result<Vec<MpcScalar<_, _>>, MpcNetworkError>>()
        .map_err(|err| format!("Error sharing coefficients: {:?}", err))?;

    let res = MpcScalar::linear_combination(&shared_values, &shared_coeffs)
        .map_err(|err| format!("Error computing linear combination: {:?}", err))?
        .open()
        .map_err(|err| format!("Error openign linear combination result: {:?}", err))?;

    // The expected value
    let linear_comb = (1..6).zip(7..12).fold(0, |acc, val| acc + val.0 * val.1);

    let expected = MpcScalar::from_public_u64(
        linear_comb,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    if res.eq(&expected) {
        Ok(())
    } else {
        Err(format!(
            "Expected: {:?}\nGot: {:?}\n",
            expected.value(),
            res.value()
        ))
    }
}

/// Tests a random linear combination
fn test_random_linear_comb(test_args: &IntegrationTestArgs) -> Result<(), String> {
    // Parties take turns allocating coefficients and values
    let n = 15;
    let mut rng = thread_rng();

    let mut values = Vec::new();
    let mut coeffs = Vec::new();
    for i in 0..n {
        values.push(
            MpcScalar::from_private_u64(
                (rng.next_u32() / 2) as u64,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
            .share_secret(i % 2 /* party_id */)
            .unwrap(),
        );

        coeffs.push(
            MpcScalar::from_private_u64(
                (rng.next_u32() / 2) as u64,
                test_args.net_ref.clone(),
                test_args.beaver_source.clone(),
            )
            .share_secret(1 - (i % 2) /* party_id */)
            .unwrap(),
        );
    }

    // Compute linear combination
    let res = MpcScalar::linear_combination(&values, &coeffs)
        .map_err(|err| format!("Error computing linear combination: {:?}", err))?
        .open()
        .map_err(|err| format!("Error opening linear combination result: {:?}", err))?;

    // Open the coeffs and scalars to compute the expected result
    let opened_scalars = MpcScalar::batch_open(&values)
        .map_err(|err| format!("Error opening values: {:?}", err))?
        .iter()
        .map(|scalar| scalar_to_u64(&scalar.to_scalar()))
        .collect::<Vec<_>>();
    let opened_coeffs = MpcScalar::batch_open(&coeffs)
        .map_err(|err| format!("Error opening coeffs: {:?}", err))?
        .iter()
        .map(|scalar| scalar_to_u64(&scalar.to_scalar()))
        .collect::<Vec<_>>();

    let mut expected_res = 0u128;
    for (scalar, coeff) in opened_scalars.iter().zip(opened_coeffs.iter()) {
        expected_res += (scalar * coeff) as u128;
    }

    if res.to_scalar().ne(&Scalar::from(expected_res)) {
        return Err(format!(
            "Expected {:?}, got {:?}",
            expected_res,
            scalar_to_u64(&res.to_scalar())
        ));
    }

    Ok(())
}

/// Each party inputs their party_id + 1 and the two together compute the square
/// Party IDs are 0 and 1, so the expected result is (0 + 1 + 1 + 1)^2 = 9
fn test_simple_mpc(test_args: &IntegrationTestArgs) -> Result<(), String> {
    let value = MpcScalar::from_private_u64(
        test_args.party_id,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    // Construct secret shares from the owned value
    let shared_value1 = value
        .share_secret(0)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;
    let shared_value2 = value
        .share_secret(1)
        .map_err(|err| format!("Error sharing value: {:?}", err))?;

    // Add one to each value
    let shared_value1 = shared_value1 + Scalar::from(1u8);
    let shared_value2 = shared_value2 + Scalar::from(1u8);

    let sum = shared_value1 + shared_value2;
    let sum_squared = &sum * &sum;

    // Open the value, assert that it equals 9
    let res = sum_squared
        .open()
        .map_err(|err| format!("Error opening: {:?}", err))?;
    let expected = MpcScalar::from_public_u64(
        9,
        test_args.net_ref.clone(),
        test_args.beaver_source.clone(),
    );

    if res.eq(&expected) {
        Ok(())
    } else {
        Err(format!(
            "Result does not equal expected\n\tResult: {:?}\n\tExpected: {:?}",
            res.value(),
            expected.value()
        ))
    }
}

// Register the tests
inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_add",
    test_fn: test_add,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_sub",
    test_fn: test_sub,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_mul",
    test_fn: test_mul
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_batch_mul",
    test_fn: test_batch_mul,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_open_value",
    test_fn: test_open_value,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_commit_and_open",
    test_fn: test_commit_and_open,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_open_batch",
    test_fn: test_open_batch,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_commit_and_open_batch",
    test_fn: test_commit_and_open_batch,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_receive_value",
    test_fn: test_receive_value,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_sum",
    test_fn: test_sum,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_product",
    test_fn: test_product,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_linear_combination",
    test_fn: test_linear_combination,
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_random_linear_comb",
    test_fn: test_random_linear_comb
});

inventory::submit!(IntegrationTest {
    name: "mpc-scalar::test_simple_mpc",
    test_fn: test_simple_mpc,
});

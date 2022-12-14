mod authenticated_ristretto;
mod authenticated_scalar;
mod mpc_ristretto;
mod mpc_scalar;
mod network;

use std::{borrow::Borrow, cell::RefCell, net::SocketAddr, process::exit, rc::Rc};

use clap::Parser;
use colored::Colorize;
use curve25519_dalek::{constants, ristretto::RistrettoPoint, scalar::Scalar};
use dns_lookup::lookup_host;

use ::mpc_ristretto::{
    mpc_scalar::MpcScalar,
    network::{MpcNetwork, QuicTwoPartyNet},
};
use mpc_scalar::PartyIDBeaverSource;

/// Integration test arguments, common to all tests
#[derive(Clone, Debug)]
struct IntegrationTestArgs {
    party_id: u64,
    net_ref: Rc<RefCell<QuicTwoPartyNet>>,
    beaver_source: Rc<RefCell<PartyIDBeaverSource>>,
    mac_key: MpcScalar<QuicTwoPartyNet, PartyIDBeaverSource>,
}

/// Integration test format
#[derive(Clone)]
struct IntegrationTest {
    pub name: &'static str,
    pub test_fn: fn(&IntegrationTestArgs) -> Result<(), String>,
}

// Collect the statically defined tests into an interable
inventory::collect!(IntegrationTest);

/// The command line interface for the test harness
#[derive(Parser, Debug)]
struct Args {
    /// The party id of the
    #[clap(long, value_parser)]
    party: u64,
    /// The port to accept inbound on
    #[clap(long = "port1", value_parser)]
    port1: u64,
    /// The port to expect the counterparty on
    #[clap(long = "port2", value_parser)]
    port2: u64,
    /// The test to run
    #[clap(short, long, value_parser)]
    test: Option<String>,
    /// Whether running in docker or not, used for peer lookup
    #[clap(long, takes_value = false, value_parser)]
    docker: bool,
}

#[allow(unused_doc_comments, clippy::await_holding_refcell_ref)]
#[tokio::main]
async fn main() {
    /**
     * Setup
     */
    let args = Args::parse();

    // Listen on 0.0.0.0 (all network interfaces) with the given port
    // We do this because listening on localhost when running in a container points to
    // the container's loopback interface, not the docker bridge
    let local_addr: SocketAddr = format!("0.0.0.0:{}", args.port1).parse().unwrap();

    // If the code is running in a docker compose setup (set by the --docker flag); attempt
    // to lookup the peer via DNS. The compose networking interface will add an alias for
    // party0 for the first peer and party1 for the second.
    // If not running on docker, dial the peer directly on the loopback interface.
    let peer_addr: SocketAddr = {
        if args.docker {
            let other_host_alias = format!("party{}", if args.party == 1 { 0 } else { 1 });
            let hosts = lookup_host(other_host_alias.as_str()).unwrap();

            println!(
                "Lookup successful for {}... found hosts: {:?}",
                other_host_alias, hosts
            );

            format!("{}:{}", hosts[0], args.port2).parse().unwrap()
        } else {
            format!("{}:{}", "127.0.0.1", args.port2).parse().unwrap()
        }
    };

    println!("Lookup successful, found peer at {:?}", peer_addr);

    // Build and connect to the network
    let mut net = QuicTwoPartyNet::new(args.party, local_addr, peer_addr);

    net.connect().await.unwrap();

    // Share the global mac key (hardcoded to Scalar(15))
    let net_ref = Rc::new(RefCell::new(net));
    let beaver_source = Rc::new(RefCell::new(PartyIDBeaverSource::new(args.party)));

    let mac_key = MpcScalar::from_private_u64(15, net_ref.clone(), beaver_source.clone())
        .share_secret(0 /* party_id */)
        .unwrap();

    /**
     * Test harness
     */
    if args.party == 0 {
        println!("\n\n{}\n", "Running integration tests...".blue());
    }

    let test_args = IntegrationTestArgs {
        party_id: args.party,
        net_ref,
        beaver_source,
        mac_key,
    };

    let mut all_success = true;

    for test in inventory::iter::<IntegrationTest> {
        if args.borrow().test.is_some() && args.borrow().test.as_deref().unwrap() != test.name {
            continue;
        }

        if args.party == 0 {
            print!("Running {}... ", test.name);
        }
        let res: Result<(), String> = (test.test_fn)(&test_args);
        all_success &= validate_success(res, args.party);
    }

    // Close the network
    #[allow(unused_must_use)]
    if test_args
        .net_ref
        .as_ref()
        .borrow_mut()
        .close()
        .await
        .is_err()
    {
        println!("Error tearing down connection");
    }

    if all_success {
        if args.party == 0 {
            println!("\n{}", "Integration tests successful!".green(),);
        }

        exit(0);
    }

    exit(-1);
}

/// Computes a * G where G is the generator of the Ristretto group
#[inline]
pub(crate) fn base_point_mul(a: u64) -> RistrettoPoint {
    constants::RISTRETTO_BASEPOINT_POINT * Scalar::from(a)
}

/// Prints a success or failure message, returns true if success, false if failure
#[inline]
fn validate_success(res: Result<(), String>, party_id: u64) -> bool {
    if res.is_ok() {
        if party_id == 0 {
            println!("{}", "Success!".green());
        }

        true
    } else {
        println!("{}\n\t{}", "Failure...".red(), res.err().unwrap());
        false
    }
}

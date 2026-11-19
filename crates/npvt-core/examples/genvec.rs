// Cross-language test-vector generator: writes v2 containers encoded by
// the Rust implementation so the C# port can prove byte-level interop.
//
//     cargo run --example genvec -- <output-dir>
//
// The samples use obviously-fake credentials; committing them as fixtures
// is safe and makes the interop contract explicit in both repos.

use npvt_core::npvt::v2::{encode, signing_key_from_seed, x25519_public_from_secret, Lock, Policy};
use npvt_core::profile::{Credential, Profile, Protocol, Transport};

fn sample() -> Profile {
    Profile {
        id: "0123abcd0123abcd".into(),
        name: "shop bundle".into(),
        protocol: Protocol::Vless,
        server: "shop.example.com".into(),
        port: 443,
        credential: Credential::Uuid("11111111-2222-3333-4444-555555555555".into()),
        transport: Transport::Tls,
        sni: Some("shop.example.com".into()),
        path: None,
        allow_insecure: false,
        udp: true,
        group: None,
        created_at: 1_700_000_000,
    }
}

fn sign_key() -> npvt_core::npvt::v2::SigningKey {
    signing_key_from_seed(&[7u8; 32])
}

fn main() {
    let out_dir = std::env::args().nth(1).expect("usage: genvec <output-dir>");
    let profile = sample();

    let open =
        encode(&profile, &Policy::default(), &Lock::Open, Some(&sign_key())).expect("encode open");
    std::fs::write(format!("{out_dir}/rust-open.npvt2"), open).expect("write open");

    let locked = encode(
        &profile,
        &Policy::default(),
        &Lock::Passphrase("correct horse".into()),
        Some(&sign_key()),
    )
    .expect("encode passphrase");
    std::fs::write(format!("{out_dir}/rust-passphrase.npvt2"), locked).expect("write passphrase");

    // Lock::Keys takes recipient PUBLIC keys; the matching secret [9u8;32]
    // lives in the C# interop tests.
    let recipient_pub = x25519_public_from_secret(&[9u8; 32]);
    let keyed = encode(
        &profile,
        &Policy {
            expires_at: Some(1_900_000_000),
            metered_only: true,
            group_hint: Some("work".into()),
        },
        &Lock::Keys(vec![recipient_pub]),
        Some(&sign_key()),
    )
    .expect("encode keys");
    std::fs::write(format!("{out_dir}/rust-keys.npvt2"), keyed).expect("write keys");

    println!("vectors written to {out_dir}");
}

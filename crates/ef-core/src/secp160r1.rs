//! Minimal SECP160r1 (SEC 2) elliptic-curve arithmetic + ECDH.
//!
//! The EcoFlow BLE V2 handshake performs an ephemeral ECDH on this curve. No mature
//! Rust crate ships secp160r1, so we implement the affine short-Weierstrass math over
//! the prime field with `num-bigint`. This is a PoC: correctness first, NOT constant
//! time. Do not reuse for anything where side channels matter without hardening.
//!
//! Wire encodings (verified empirically against python-ecdsa):
//!   * public key  = X(20 bytes BE) || Y(20 bytes BE)          = 40 bytes
//!   * ECDH secret = X of (d * peerPub), 20 bytes BE (padded to p length)

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Zero};
use rand::RngCore;

const FIELD_BYTES: usize = 20;

fn h(s: &str) -> BigInt {
    BigInt::parse_bytes(s.as_bytes(), 16).unwrap()
}

/// Domain parameters for secp160r1.
struct Params {
    p: BigInt,
    a: BigInt,
    #[allow(dead_code)] // read only by the on-curve unit test
    b: BigInt,
    n: BigInt,
    gx: BigInt,
    gy: BigInt,
}

fn params() -> Params {
    Params {
        p: h("ffffffffffffffffffffffffffffffff7fffffff"),
        a: h("ffffffffffffffffffffffffffffffff7ffffffc"),
        b: h("1c97befc54bd7a8b65acf89f81d4d4adc565fa45"),
        n: h("100000000000000000001f4c8f927aed3ca752257"),
        gx: h("4a96b5688ef573284664698968c38bb913cbfc82"),
        gy: h("23a628553168947d59dcc912042351377ac5fb32"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Point {
    Infinity,
    Affine(BigInt, BigInt),
}

fn modp(x: &BigInt, p: &BigInt) -> BigInt {
    x.mod_floor(p)
}

/// Modular inverse via Fermat's little theorem (p is prime): a^(p-2) mod p.
fn mod_inv(a: &BigInt, p: &BigInt) -> BigInt {
    let base = modp(a, p);
    let exp = p - 2;
    base.modpow(&exp, p)
}

fn point_add(pt1: &Point, pt2: &Point, prm: &Params) -> Point {
    let p = &prm.p;
    match (pt1, pt2) {
        (Point::Infinity, q) => q.clone(),
        (q, Point::Infinity) => q.clone(),
        (Point::Affine(x1, y1), Point::Affine(x2, y2)) => {
            // P + (-P) = O
            if x1 == x2 && modp(&(y1 + y2), p).is_zero() {
                return Point::Infinity;
            }
            let lambda = if x1 == x2 && y1 == y2 {
                // doubling: (3*x1^2 + a) / (2*y1)
                let num = modp(&(BigInt::from(3) * x1 * x1 + &prm.a), p);
                let den = mod_inv(&(BigInt::from(2) * y1), p);
                modp(&(num * den), p)
            } else {
                // addition: (y2 - y1) / (x2 - x1)
                let num = modp(&(y2 - y1), p);
                let den = mod_inv(&modp(&(x2 - x1), p), p);
                modp(&(num * den), p)
            };
            let x3 = modp(&(&lambda * &lambda - x1 - x2), p);
            let y3 = modp(&(&lambda * (x1 - &x3) - y1), p);
            Point::Affine(x3, y3)
        }
    }
}

fn scalar_mul(k: &BigInt, pt: &Point, prm: &Params) -> Point {
    let mut result = Point::Infinity;
    let mut addend = pt.clone();
    let mut kk = k.clone();
    let zero = BigInt::zero();
    let one = BigInt::one();
    while kk > zero {
        if (&kk & &one) == one {
            result = point_add(&result, &addend, prm);
        }
        addend = point_add(&addend, &addend, prm);
        kk >>= 1;
    }
    result
}

fn to_field_bytes(v: &BigInt) -> [u8; FIELD_BYTES] {
    // big-endian, left-padded to 20 bytes
    let (_, mut bytes) = v.to_bytes_be();
    let mut out = [0u8; FIELD_BYTES];
    if bytes.len() > FIELD_BYTES {
        // shouldn't happen for a valid field element; keep the low 20 bytes
        bytes = bytes[bytes.len() - FIELD_BYTES..].to_vec();
    }
    out[FIELD_BYTES - bytes.len()..].copy_from_slice(&bytes);
    out
}

/// An ephemeral secret scalar + its 40-byte public key.
pub struct Ephemeral {
    d: BigInt,
    pub public_key: [u8; 40],
}

/// Generate an ephemeral keypair (d in [1, n-1], public = d*G).
pub fn generate() -> Ephemeral {
    let prm = params();
    let d = random_scalar(&prm.n);
    let g = Point::Affine(prm.gx.clone(), prm.gy.clone());
    let pubp = scalar_mul(&d, &g, &prm);
    let (x, y) = match pubp {
        Point::Affine(x, y) => (x, y),
        Point::Infinity => unreachable!("d*G is not infinity for d in [1,n-1]"),
    };
    let mut public_key = [0u8; 40];
    public_key[..20].copy_from_slice(&to_field_bytes(&x));
    public_key[20..].copy_from_slice(&to_field_bytes(&y));
    Ephemeral { d, public_key }
}

impl Ephemeral {
    /// Compute the ECDH shared secret with a peer's 40-byte public key.
    /// Returns the 20-byte big-endian X coordinate of d*peerPub.
    pub fn shared_secret(&self, peer_pub: &[u8]) -> anyhow::Result<[u8; 20]> {
        if peer_pub.len() < 40 {
            anyhow::bail!("peer public key too short: {} bytes", peer_pub.len());
        }
        let prm = params();
        let px = BigInt::from_bytes_be(num_bigint::Sign::Plus, &peer_pub[..20]);
        let py = BigInt::from_bytes_be(num_bigint::Sign::Plus, &peer_pub[20..40]);
        let peer = Point::Affine(px, py);
        let shared = scalar_mul(&self.d, &peer, &prm);
        match shared {
            Point::Affine(x, _) => Ok(to_field_bytes(&x)),
            Point::Infinity => anyhow::bail!("ECDH produced point at infinity"),
        }
    }
}

fn random_scalar(n: &BigInt) -> BigInt {
    // n is 161 bits; sample 21 bytes, reduce into [1, n-1].
    let mut rng = rand::rngs::OsRng;
    loop {
        let mut buf = [0u8; 21];
        rng.fill_bytes(&mut buf);
        let candidate = BigInt::from_bytes_be(num_bigint::Sign::Plus, &buf);
        let reduced = candidate.mod_floor(n);
        if !reduced.is_zero() {
            return reduced;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_on_curve() {
        // y^2 == x^3 + a*x + b (mod p)
        let prm = params();
        let lhs = modp(&(&prm.gy * &prm.gy), &prm.p);
        let rhs = modp(&(&prm.gx * &prm.gx * &prm.gx + &prm.a * &prm.gx + &prm.b), &prm.p);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn order_kills_generator() {
        // n * G == O
        let prm = params();
        let g = Point::Affine(prm.gx.clone(), prm.gy.clone());
        assert_eq!(scalar_mul(&prm.n, &g, &prm), Point::Infinity);
    }

    #[test]
    fn ecdh_is_symmetric() {
        let a = generate();
        let b = generate();
        let sa = a.shared_secret(&b.public_key).unwrap();
        let sb = b.shared_secret(&a.public_key).unwrap();
        assert_eq!(sa, sb);
    }
}

use zcash_address::unified::{Encoding, Ufvk};
use zcash_protocol::consensus::{NetworkUpgrade, NetworkType};
use zcash_protocol::consensus::{BlockHeight, MainNetwork, Parameters};

pub const UFVK_PREFIX: &str = "uview1";
pub const CHAIN_LABEL: &str = "mainnet";
pub const BLOCK_SPACING_SECS: u64 = 75;

pub fn sapling_activation() -> u32 {
    u32::from(
        MainNetwork
            .activation_height(NetworkUpgrade::Sapling)
            .expect("mainnet activates Sapling"),
    )
}

pub fn check_ufvk(ufvk: &str) -> Result<(), String> {
    if ufvk.starts_with("zxview") {
        return Err(
            "a legacy Sapling key · export the unified key (uview1…) from the app".into(),
        );
    }
    if ufvk.starts_with("u1") || ufvk.starts_with("utest1") {
        return Err(
            "that is an address, not a viewing key · in the app: settings › viewing key".into(),
        );
    }
    let ufvk = ufvk.trim();
    if ufvk.starts_with("uviewtest1") {
        return Err(
            "a testnet key · this wallet is for the Zcash main chain, whose keys start with uview1"
                .into(),
        );
    }
    if ufvk.starts_with("uviewregtest1") {
        return Err(
            "a regtest key · this wallet is for the Zcash main chain, whose keys start with uview1"
                .into(),
        );
    }
    match Ufvk::decode(ufvk) {
        Ok((NetworkType::Main, _, _)) => Ok(()),
        Ok(_) => Err("a key for another chain · this wallet is mainnet only".into()),
        Err(_) => Err(format!(
            "not a unified full viewing key: expected a prefix of {UFVK_PREFIX}"
        )),
    }
}

pub fn block_height(height: u32) -> BlockHeight {
    BlockHeight::from_u32(height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sapling_activated_where_the_consensus_chain_says() {
        assert_eq!(sapling_activation(), 419_200);
    }

    #[test]
    fn only_mainnet_keys_pass_the_check() {
        assert!(check_ufvk("uview1cgrqnry478ckvpr0f580t6fsahp0a5mj2e9xl7hv2d2jd4ldzy449mwwk2l9yeuts85wjls6hjtghdsy5vhhvmjdw3jxl3cxhrg3vs296a3czazrycrr5cywjhwc5c3ztfyjdhmz0exvzzeyejamyp0cr9z8f9wj0953fzht0m4lenk94t70ruwgjxag2tvp63wn9ftzhtkh20gyre3w5s24f6wlgqxnjh40gd2lxe75sf3z8h5y2x0atpxcyf9t3em4h0evvsftluruqne6w4sm066sw0qe5y8qg423grple5fftxrqyy7xmqmatv7nzd7tcjadu8f7mqz4l83jsyxy4t8pkayytyk7nrp467ds85knekdkvnd7hqkfer8mnqd7pv").is_ok());
        assert!(check_ufvk("zxviewtestsaplingkey").unwrap_err().contains("legacy"));
        assert!(check_ufvk("u1mainnetaddress").unwrap_err().contains("address"));
        assert!(check_ufvk("notakey").unwrap_err().contains("not a unified"));
    }
}

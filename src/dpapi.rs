//! Windows DPAPI wrappers (CryptProtectData / CryptUnprotectData).
//! Secrets are encrypted with the current user's master key so that the
//! configuration file is useless if copied to another machine/account.

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

fn as_blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

unsafe fn take_blob(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let mut v = Vec::with_capacity(out.cbData as usize);
    if !out.pbData.is_null() && out.cbData > 0 {
        v.extend_from_slice(std::slice::from_raw_parts(out.pbData, out.cbData as usize));
    }
    if !out.pbData.is_null() {
        LocalFree(out.pbData as *mut core::ffi::c_void);
    }
    v
}

pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
    if plain.is_empty() {
        return Ok(Vec::new());
    }
    let input = as_blob(plain);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        bail!("DPAPI CryptProtectData failed");
    }
    Ok(unsafe { take_blob(&output) })
}

pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>> {
    if cipher.is_empty() {
        return Ok(Vec::new());
    }
    let input = as_blob(cipher);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        bail!("DPAPI CryptUnprotectData failed (profile may belong to another user)");
    }
    Ok(unsafe { take_blob(&output) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = b"12345678-1234-1234-1234-123456789abc";
        let enc = protect(secret).expect("protect");
        assert_ne!(&enc[..], &secret[..]);
        let dec = unprotect(&enc).expect("unprotect");
        assert_eq!(&dec[..], &secret[..]);
    }
}

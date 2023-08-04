use std::{fs, io::Write, path::PathBuf, str::FromStr};

use age::secrecy::ExposeSecret;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const KEY_FILE_PATH: &str = "private_key";
const ENC_FILE_PATHS: [&str; 2] = ["config.toml", "credentials.json"];

pub fn main() -> Result {
    let private_key_path: PathBuf = KEY_FILE_PATH.into();

    let private_key = if private_key_path.exists() {
        let key_str = fs::read_to_string(KEY_FILE_PATH)?;
        age::x25519::Identity::from_str(&key_str)?
    } else {
        let key = age::x25519::Identity::generate();
        fs::write(private_key_path, key.to_string().expose_secret())?;
        key
    };

    let public_key = private_key.to_public();

    for path in ENC_FILE_PATHS {
        let file = fs::read_to_string(path)?;

        let encryptor =
            age::Encryptor::with_recipients(vec![Box::new(public_key.clone())]).unwrap();

        let mut encrypted = vec![];

        let mut writer = encryptor.wrap_output(&mut encrypted)?;
        writer.write_all(file.as_bytes())?;
        writer.finish()?;

        fs::write(format!("{}.enc", path), encrypted)?;
    }

    Ok(())
}

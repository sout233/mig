use aes::cipher::{BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit, block_padding::Pkcs7};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::io::{Read, Write};

fn bytes(value: &Value) -> Result<Vec<u8>, String> {
    value
        .as_array()
        .ok_or_else(|| "需要字节数组".into())
        .map(|array| {
            array
                .iter()
                .map(|value| value.as_u64().unwrap_or(0) as u8)
                .collect()
        })
}

pub(super) fn call(operation: &str, input: Value) -> Result<Value, String> {
    match operation {
        "encode" => {
            let value = input["value"].as_str().unwrap_or_default();

            let bytes = match input["encoding"]
                .as_str()
                .unwrap_or("utf8")
                .to_ascii_lowercase()
                .as_str()
            {
                "base64" => STANDARD.decode(value).map_err(|e| e.to_string())?,
                "hex" => {
                    if !value.len().is_multiple_of(2) || !value.is_ascii() {
                        return Err("无效十六进制数据".into());
                    }
                    (0..value.len())
                        .step_by(2)
                        .map(|i| {
                            u8::from_str_radix(&value[i..i + 2], 16).map_err(|e| e.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?
                }
                "binary" | "latin1" | "ascii" => value.chars().map(|char| char as u8).collect(),
                "utf8" | "utf-8" => value.as_bytes().to_vec(),
                encoding => return Err(format!("不支持编码 {encoding}")),
            };

            Ok(json!(bytes))
        }
        "decode" => {
            let bytes = bytes(&input["bytes"])?;
            Ok(json!(match input["encoding"]
                .as_str()
                .unwrap_or("utf8")
                .to_ascii_lowercase()
                .as_str()
            {
                "base64" => STANDARD.encode(bytes),
                "hex" => bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
                "binary" | "latin1" | "ascii" =>
                    bytes.iter().map(|byte| char::from(*byte)).collect(),
                "utf8" | "utf-8" => String::from_utf8_lossy(&bytes).into_owned(),
                encoding => return Err(format!("不支持编码 {encoding}")),
            }))
        }
        "md5" => {
            use md5::{Digest, Md5};
            Ok(json!(format!("{:x}", Md5::digest(bytes(&input)?))))
        }
        "random" => {
            let length = input
                .as_u64()
                .filter(|length| *length <= 1024 * 1024)
                .ok_or("随机字节长度无效")?;

            let mut bytes = vec![0; length as usize];
            getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
            Ok(json!(bytes))
        }
        "deflate" => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());

            encoder
                .write_all(&bytes(&input)?)
                .map_err(|e| e.to_string())?;
            Ok(json!(encoder.finish().map_err(|e| e.to_string())?))
        }
        "inflate" => {
            let data = bytes(&input)?;
            let mut output = Vec::new();
            flate2::read::ZlibDecoder::new(data.as_slice())
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut output)
                .map_err(|e| e.to_string())?;
            if output.len() > 4 * 1024 * 1024 {
                return Err("解压数据超过大小限制".into());
            }
            Ok(json!(output))
        }
        "aes" => aes_encrypt(&input),
        "rsa" => {
            use rsa::{
                BigUint, RsaPublicKey, pkcs1::DecodeRsaPublicKey, pkcs8::DecodePublicKey,
                traits::PublicKeyParts,
            };
            let pem = input["key"].as_str().ok_or("RSA 公钥无效")?;
            let key = RsaPublicKey::from_public_key_pem(pem)
                .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
                .map_err(|e| e.to_string())?;

            let number = BigUint::from_bytes_be(&bytes(&input["bytes"])?);
            if &number >= key.n() {
                return Err("RSA 数据过长".into());
            }
            let output = number.modpow(key.e(), key.n()).to_bytes_be();
            let mut padded = vec![0; key.size() - output.len()];
            padded.extend(output);
            Ok(json!(padded))
        }
        "url" => {
            let address = input["url"].as_str().ok_or("URL 无效")?;

            let url = if let Some(base) = input["base"].as_str() {
                reqwest::Url::parse(base).and_then(|base| base.join(address))
            } else {
                reqwest::Url::parse(address)
            }
            .map_err(|e| e.to_string())?;

            Ok(
                json!({"protocol":format!("{}:",url.scheme()),"host":url[url::Position::BeforeHost..url::Position::AfterPort],
                "hostname":url.host_str(),"pathname":url.path(),"search":url.query().map(|q|format!("?{q}")).unwrap_or_default(),
                "hash":url.fragment().map(|q|format!("#{q}")).unwrap_or_default()}),
            )
        }
        _ => Err(format!("不支持的脚本 API：{operation}")),
    }
}

fn aes_encrypt(input: &Value) -> Result<Value, String> {
    let data = bytes(&input["bytes"])?;
    let key = bytes(&input["key"])?;
    let iv = bytes(&input["iv"])?;

    let mode = input["mode"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();

    macro_rules! encrypt {
        ($cipher:ty) => {{
            if mode.ends_with("-cbc") {
                cbc::Encryptor::<$cipher>::new_from_slices(&key, &iv)
                    .map_err(|e| e.to_string())?
                    .encrypt_padded_vec_mut::<Pkcs7>(&data)
            } else if mode.ends_with("-ecb") {
                let cipher = <$cipher>::new_from_slice(&key).map_err(|e| e.to_string())?;
                let padding = 16 - data.len() % 16;
                let mut output = data.clone();
                output.extend(std::iter::repeat_n(padding as u8, padding));
                for block in output.chunks_exact_mut(16) {
                    cipher.encrypt_block(block.into());
                }
                output
            } else {
                return Err(format!("不支持 AES 模式 {mode}"));
            }
        }};
    }
    Ok(json!(match key.len() {
        16 => encrypt!(aes::Aes128),
        24 => encrypt!(aes::Aes192),
        32 => encrypt!(aes::Aes256),
        _ => return Err("AES 密钥长度无效".into()),
    }))
}

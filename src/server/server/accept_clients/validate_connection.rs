use crate::message::{Message, MessageHeader, MessageType};

use rand::rngs::OsRng;
use rsa::{PaddingScheme, PublicKeyParts, RSAPrivateKey, RSAPublicKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// The validation flow is like this
//
// 1. Client connects
// 2. Server generates and sends public key
// 3. Client sends encrypted password/key
// 4. Server decrypts the message and checks if the password/key is valid
// 5a. If valid: Server sends an Acknowledge message and its done
// 5b. If invalid: Server closes the connection
pub async fn validate_connection(con: &mut tokio::net::TcpStream, key: &[u8]) -> bool {
    // Step 2
    let mut rng = OsRng;
    let priv_key = RSAPrivateKey::new(&mut rng, 2048).expect("Failed to generate private key");
    let pub_key = RSAPublicKey::from(&priv_key);

    let pub_n_bytes = pub_key.n().to_bytes_le();
    let mut pub_e_bytes = pub_key.e().to_bytes_le();

    let mut data = pub_n_bytes;
    data.append(&mut pub_e_bytes);

    let msg_header = MessageHeader::new(0, MessageType::Key, data.len() as u64);
    let msg = Message::new(msg_header, data);

    let data = msg.serialize();
    match con.write_all(&data).await {
        Ok(_) => {}
        Err(e) => {
            println!("Error sending Key: {}", e);
            return false;
        }
    };

    // Step 4
    let mut head_buf = [0; 13];
    let header = match con.read_exact(&mut head_buf).await {
        Ok(_) => {
            let msg = MessageHeader::deserialize(head_buf);
            if msg.is_none() {
                return false;
            }
            msg.unwrap()
        }
        Err(e) => {
            println!("Error reading validate: {}", e);
            return false;
        }
    };
    if *header.get_kind() != MessageType::Verify {
        return false;
    }

    let key_length = header.get_length() as usize;
    let mut recv_encrypted_key = vec![0; key_length];
    match con.read_exact(&mut recv_encrypted_key).await {
        Ok(_) => {}
        Err(e) => {
            println!("Could not read key: {}", e);
            return false;
        }
    };

    let recv_key = match priv_key.decrypt(PaddingScheme::PKCS1v15Encrypt, &recv_encrypted_key) {
        Ok(raw_key) => raw_key,
        Err(e) => {
            println!("Error decrypting received-key: {}", e);
            return false;
        }
    };

    // Step 5
    if recv_key != key {
        // Step 5a
        println!("The keys are not matching");
        return false;
    }

    // Step 5b
    let ack_header = MessageHeader::new(0, MessageType::Acknowledge, 0);
    let ack_data = ack_header.serialize();
    match con.write_all(&ack_data).await {
        Ok(_) => {}
        Err(e) => {
            println!("Error sending acknowledge: {}", e);
            return false;
        }
    };

    true
}

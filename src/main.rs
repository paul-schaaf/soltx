use {
    clap::{App, Arg},
    solana_clap_utils::{input_validators::is_valid_signer, keypair::DefaultSigner},
    solana_client::rpc_client::RpcClient,
    solana_remote_wallet::remote_wallet::RemoteWalletManager,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::{Signature, Signer},
        transaction::Transaction,
    },
    std::{process::exit, sync::Arc},
};

use std::{fs};

use anyhow::Result;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use yaml_rust::{Yaml, YamlLoader};
struct Config {
    commitment_config: CommitmentConfig,
    default_signer: Box<dyn Signer>,
    json_rpc_url: String,
}

fn main() -> Result<()> {
    let matches = App::new("soltx")
        .arg(Arg::with_name("FILE").required(true))
        .arg({
            let arg = Arg::with_name("config_file")
                .short("C")
                .long("config")
                .value_name("CONFIG_PATH")
                .takes_value(true)
                .help("Configuration file to use");
            if let Some(ref config_file) = *solana_cli_config::CONFIG_FILE {
                arg.default_value(&config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::with_name("keypair")
                .long("keypair")
                .value_name("KEYPAIR")
                .validator(is_valid_signer)
                .takes_value(true)
                .global(true)
                .help("Filepath or URL to a keypair [default: client keypair]"),
        )
        .get_matches();

    let mut wallet_manager: Option<Arc<RemoteWalletManager>> = None;

    let config = {
        let cli_config = if let Some(config_file) = matches.value_of("config_file") {
            solana_cli_config::Config::load(config_file).unwrap_or_default()
        } else {
            solana_cli_config::Config::default()
        };

        let default_signer = DefaultSigner {
            path: matches
                .value_of(&"keypair")
                .map(|s| s.to_string())
                .unwrap_or_else(|| cli_config.keypair_path.clone()),
            arg_name: "keypair".to_string(),
        };

        Config {
            json_rpc_url: matches
                .value_of("json_rpc_url")
                .unwrap_or(&cli_config.json_rpc_url)
                .to_string(),
            default_signer: default_signer
                .signer_from_path(&matches, &mut wallet_manager)
                .unwrap_or_else(|err| {
                    eprintln!("error: {}", err);
                    exit(1);
                }),
            commitment_config: CommitmentConfig::confirmed(),
        }
    };

    let rpc_client = RpcClient::new(config.json_rpc_url.clone());

    // unwrap OK cause FILE is a required arg
    if let Some(path) = matches.value_of("FILE") {
        let file_content = fs::read_to_string(path)?;
        let content_as_yaml = YamlLoader::load_from_str(&file_content)?;
        let signature = send_transaction(
            content_as_yaml.get(0),
            config.default_signer.as_ref(),
            &rpc_client,
            config.commitment_config,
        )?;
        println!("{}", signature);
    }
    Ok(())
}

fn send_transaction(
    yaml: Option<&Yaml>,
    signer: &dyn Signer,
    rpc_client: &RpcClient,
    commitment_config: CommitmentConfig,
) -> Result<Signature> {
    let instructions = match yaml {
        None => vec![],
        Some(v) =>     v
        .as_vec()
        .unwrap()
        .iter()
        .map(|x| yaml_to_instruction(x))
        .collect::<Vec<Instruction>>()
    };
    
    let mut transaction =
        Transaction::new_with_payer(instructions.as_slice(), Some(&signer.pubkey()));
    let (recent_blockhash, _fee_calculator) = rpc_client.get_recent_blockhash()?;

    transaction.try_sign(&vec![signer], recent_blockhash)?;

    println!("{:?}", &transaction.signatures);

    let signature = rpc_client.send_and_confirm_transaction_with_spinner_and_config(
        &transaction,
        commitment_config,
        RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: None,
            encoding: None
        }
    )?;
    Ok(signature)
}

fn yaml_to_account_meta(yaml: &Yaml) -> AccountMeta {
    AccountMeta {
        pubkey: yaml["key"].as_str().unwrap().parse().unwrap(),
        is_signer: yaml["isSigner"].as_bool().unwrap(),
        is_writable: yaml["isWritable"].as_bool().unwrap(),
    }
}

fn yaml_to_instruction(yaml: &Yaml) -> Instruction {
    let data = yaml["data"]
        .as_str()
        .unwrap()
        .split(',')
        .map(|x| x.parse::<u8>().unwrap())
        .collect();
    let accounts = yaml["accounts"]
        .as_vec()
        .unwrap()
        .iter()
        .map(|x| yaml_to_account_meta(x))
        .collect();
    Instruction {
        program_id: yaml["programId"].as_str().unwrap().parse().unwrap(),
        data,
        accounts,
    }
}

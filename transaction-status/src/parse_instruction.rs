use {
    crate::{
        extract_memos::anima_memo_id,
        parse_associated_token::{parse_associated_token, anima_associated_token_id},
        parse_stake::parse_stake,
        parse_system::parse_system,
        parse_token::parse_token,
        parse_vote::parse_vote,
    },
    inflector::Inflector,
    serde_json::Value,
    mundis_account_decoder::parse_token::anima_token_ids,
    mundis_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, stake, system_program},
    std::{
        collections::HashMap,
        str::{from_utf8, Utf8Error},
    },
    thiserror::Error,
};

lazy_static! {
    static ref ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey = anima_associated_token_id();
    static ref MEMO_PROGRAM_ID: Pubkey = anima_memo_id();
    static ref STAKE_PROGRAM_ID: Pubkey = stake::program::id();
    static ref SYSTEM_PROGRAM_ID: Pubkey = system_program::id();
    static ref VOTE_PROGRAM_ID: Pubkey = mundis_vote_program::id();
    static ref PARSABLE_PROGRAM_IDS: HashMap<Pubkey, ParsableProgram> = {
        let mut m = HashMap::new();
        m.insert(
            *ASSOCIATED_TOKEN_PROGRAM_ID,
            ParsableProgram::AnimaTokenAccount,
        );
        m.insert(*MEMO_PROGRAM_ID, ParsableProgram::AnimaMemo);
        for anima_token_id in anima_token_ids() {
            m.insert(anima_token_id, ParsableProgram::AnimaToken);
        }
        m.insert(*STAKE_PROGRAM_ID, ParsableProgram::Stake);
        m.insert(*SYSTEM_PROGRAM_ID, ParsableProgram::System);
        m.insert(*VOTE_PROGRAM_ID, ParsableProgram::Vote);
        m
    };
}

#[derive(Error, Debug)]
pub enum ParseInstructionError {
    #[error("{0:?} instruction not parsable")]
    InstructionNotParsable(ParsableProgram),

    #[error("{0:?} instruction key mismatch")]
    InstructionKeyMismatch(ParsableProgram),

    #[error("Program not parsable")]
    ProgramNotParsable,

    #[error("Internal error, please report")]
    SerdeJsonError(#[from] serde_json::error::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedInstruction {
    pub program: String,
    pub program_id: String,
    pub parsed: Value,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedInstructionEnum {
    #[serde(rename = "type")]
    pub instruction_type: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub info: Value,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ParsableProgram {
    AnimaTokenAccount,
    AnimaMemo,
    AnimaToken,
    Stake,
    System,
    Vote,
}

pub fn parse(
    program_id: &Pubkey,
    instruction: &CompiledInstruction,
    account_keys: &[Pubkey],
) -> Result<ParsedInstruction, ParseInstructionError> {
    let program_name = PARSABLE_PROGRAM_IDS
        .get(program_id)
        .ok_or(ParseInstructionError::ProgramNotParsable)?;
    let parsed_json = match program_name {
        ParsableProgram::AnimaTokenAccount => {
            serde_json::to_value(parse_associated_token(instruction, account_keys)?)?
        }
        ParsableProgram::AnimaMemo => parse_memo(instruction)?,
        ParsableProgram::AnimaToken => serde_json::to_value(parse_token(instruction, account_keys)?)?,
        ParsableProgram::Stake => serde_json::to_value(parse_stake(instruction, account_keys)?)?,
        ParsableProgram::System => serde_json::to_value(parse_system(instruction, account_keys)?)?,
        ParsableProgram::Vote => serde_json::to_value(parse_vote(instruction, account_keys)?)?,
    };
    Ok(ParsedInstruction {
        program: format!("{:?}", program_name).to_kebab_case(),
        program_id: program_id.to_string(),
        parsed: parsed_json,
    })
}

fn parse_memo(instruction: &CompiledInstruction) -> Result<Value, ParseInstructionError> {
    parse_memo_data(&instruction.data)
        .map(Value::String)
        .map_err(|_| ParseInstructionError::InstructionNotParsable(ParsableProgram::AnimaMemo))
}

pub fn parse_memo_data(data: &[u8]) -> Result<String, Utf8Error> {
    from_utf8(data).map(|s| s.to_string())
}

pub(crate) fn check_num_accounts(
    accounts: &[u8],
    num: usize,
    parsable_program: ParsableProgram,
) -> Result<(), ParseInstructionError> {
    if accounts.len() < num {
        Err(ParseInstructionError::InstructionKeyMismatch(
            parsable_program,
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use {super::*, serde_json::json};

    #[test]
    fn test_parse() {
        let memo_instruction = CompiledInstruction {
            program_id_index: 0,
            accounts: vec![],
            data: vec![240, 159, 166, 150],
        };

        assert_eq!(
            parse(&MEMO_PROGRAM_ID, &memo_instruction, &[]).unwrap(),
            ParsedInstruction {
                program: "anima-memo".to_string(),
                program_id: MEMO_PROGRAM_ID.to_string(),
                parsed: json!("🦖"),
            }
        );

        let non_parsable_program_id = Pubkey::new(&[1; 32]);
        assert!(parse(&non_parsable_program_id, &memo_instruction, &[]).is_err());
    }

    #[test]
    fn test_parse_memo() {
        let good_memo = "good memo".to_string();
        assert_eq!(
            parse_memo(&CompiledInstruction {
                program_id_index: 0,
                accounts: vec![],
                data: good_memo.as_bytes().to_vec(),
            })
            .unwrap(),
            Value::String(good_memo),
        );

        let bad_memo = vec![128u8];
        assert!(std::str::from_utf8(&bad_memo).is_err());
        assert!(parse_memo(&CompiledInstruction {
            program_id_index: 0,
            data: bad_memo,
            accounts: vec![],
        })
        .is_err(),);
    }
}

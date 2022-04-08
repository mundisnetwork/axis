use super::*;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum VoteStateVersions {
    Current(Box<VoteState>),
}

impl VoteStateVersions {
    pub fn new_current(vote_state: VoteState) -> Self {
        Self::Current(Box::new(vote_state))
    }

    pub fn convert_to_current(self) -> VoteState {
        match self {
            VoteStateVersions::Current(state) => *state,
        }
    }

    pub fn is_uninitialized(&self) -> bool {
        match self {
            VoteStateVersions::Current(vote_state) => vote_state.authorized_voters.is_empty(),
        }
    }
}

use {
    crossbeam_channel::{Receiver, Sender},
    mundis_sdk::{hash::Hash, pubkey::Pubkey},
    mundis_vote_program::vote_state::Vote,
};

pub type ReplayedVote = (Pubkey, Vote, Option<Hash>);
pub type ReplayVoteSender = Sender<ReplayedVote>;
pub type ReplayVoteReceiver = Receiver<ReplayedVote>;

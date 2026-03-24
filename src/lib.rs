#![no_std]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

pub mod errors;
use errors::*;

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SessionStatus {
    None = 0,
    Open = 1,
    Closed = 2,
}

#[type_abi]
#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, Clone, Debug)]
pub struct SessionData<M: ManagedTypeApi> {
    pub employer: ManagedAddress<M>,
    pub receiver: ManagedAddress<M>,
    pub token_identifier: EgldOrEsdtTokenIdentifier<M>,
    pub amount_locked: BigUint<M>,
    pub amount_settled: BigUint<M>,
    pub nonce: u64, // This will be the last voucher nonce
    pub deadline: u64,
    pub status: u8,
} // 0: Open, 1: Closing, 2: Closed

#[multiversx_sc::contract]
pub trait MppSessionContract {
    #[init]
    fn init(&self) {}

    #[upgrade]
    fn upgrade(&self) {}

    /// Open a new payment session.
    /// Locks the payment sent in the transaction.
    #[payable("*")]
    #[endpoint(open)]
    fn open(&self, receiver: ManagedAddress, deadline: u64) -> ManagedBuffer {
        let payment = self.call_value().egld_or_single_esdt();
        require!(payment.amount > 0u64, ERR_ZERO_DEPOSIT);

        let current_timestamp = self.blockchain().get_block_timestamp_seconds();
        require!(deadline > current_timestamp.as_u64_seconds(), ERR_DEADLINE_IN_PAST);

        let employer = self.blockchain().get_caller();
        
        // Compute unique channel_id
        let channel_nonce = self.last_channel_nonce(&employer).update(|n| {
            *n += 1;
            *n
        });

        let mut channel_id_msg = ManagedBuffer::new();
        channel_id_msg.append(employer.as_managed_buffer());
        channel_id_msg.append(receiver.as_managed_buffer());
        let nonce_bytes = channel_nonce.to_be_bytes();
        channel_id_msg.append_bytes(&nonce_bytes[..]);
        
        let channel_id = self.crypto().keccak256(&channel_id_msg);
        let channel_id_buf = channel_id.as_managed_buffer();

        let session = SessionData {
            employer: employer.clone(),
            receiver: receiver.clone(),
            token_identifier: payment.token_identifier.clone(),
            amount_locked: payment.amount.clone(),
            amount_settled: BigUint::zero(),
            nonce: 0, // last voucher nonce
            deadline,
            status: SessionStatus::Open as u8,
        };

        self.sessions(&channel_id_buf).set(&session);
        self.last_id().set(channel_id_buf.clone());
        
        self.open_session_event(
            &channel_id_buf,
            &employer,
            &receiver,
            &payment.token_identifier,
            &payment.amount,
        );

        channel_id_buf.clone()
    }

    #[payable("*")]
    #[endpoint(top_up)]
    fn top_up(&self, channel_id: ManagedBuffer) {
        let mut session = self.sessions(&channel_id).get();
        require!(session.status == SessionStatus::Open as u8, ERR_ALREADY_CLOSED);

        let payment = self.call_value().egld_or_single_esdt();
        require!(payment.token_identifier == session.token_identifier, ERR_INVALID_TOKEN);
        require!(payment.amount > 0u64, ERR_ZERO_DEPOSIT);

        session.amount_locked += &payment.amount;
        self.sessions(&channel_id).set(&session);
    }

    #[endpoint(settle)]
    fn settle(&self, channel_id: ManagedBuffer, amount: BigUint, nonce: u64, signature: ManagedBuffer) {
        let mut session = self.sessions(&channel_id).get();
        require!(session.status == SessionStatus::Open as u8, ERR_ALREADY_CLOSED);
        
        self.verify_voucher(&channel_id, &amount, nonce, &signature, &session.employer);

        require!(nonce > session.nonce, ERR_STALE_VOUCHER);
        require!(amount > session.amount_settled, ERR_INVALID_AMOUNT);
        require!(amount <= session.amount_locked, ERR_INSUFFICIENT_FUNDS);

        let to_release = &amount - &session.amount_settled;
        session.amount_settled = amount.clone();
        session.nonce = nonce;

        self.sessions(&channel_id).set(&session);

        self.send_tokens(&session.receiver, &session.token_identifier, &to_release);
        self.settle_session_event(&channel_id, nonce, &amount);
    }

    #[endpoint(close)]
    fn close(&self, channel_id: ManagedBuffer, amount: BigUint, nonce: u64, signature: ManagedBuffer) {
        let mut session = self.sessions(&channel_id).get();
        require!(session.status == SessionStatus::Open as u8, ERR_ALREADY_CLOSED);
        
        self.verify_voucher(&channel_id, &amount, nonce, &signature, &session.employer);

        require!(nonce >= session.nonce, ERR_STALE_VOUCHER);
        require!(amount >= session.amount_settled, ERR_INVALID_AMOUNT);
        require!(amount <= session.amount_locked, ERR_INSUFFICIENT_FUNDS);

        let to_release = &amount - &session.amount_settled;
        let refund = &session.amount_locked - &amount;

        session.amount_settled = amount.clone();
        session.nonce = nonce;
        session.status = SessionStatus::Closed as u8;

        self.sessions(&channel_id).set(&session);

        if to_release > 0u64 {
            self.send_tokens(&session.receiver, &session.token_identifier, &to_release);
        }

        if refund > 0u64 {
            self.send_tokens(&session.employer, &session.token_identifier, &refund);
        }

        self.close_session_event(&channel_id, &session.amount_settled, &refund);
    }

    #[endpoint(request_close)]
    fn request_close(&self, channel_id: ManagedBuffer) {
        let mut session = self.sessions(&channel_id).get();
        require!(session.status == SessionStatus::Open as u8, ERR_ALREADY_CLOSED);

        let caller = self.blockchain().get_caller();
        require!(caller == session.employer, ERR_NOT_EMPLOYER);

        let current_timestamp = self.blockchain().get_block_timestamp_seconds();
        require!(current_timestamp.as_u64_seconds() >= session.deadline, ERR_CHALLENGE_PERIOD_NOT_OVER);

        let refund = &session.amount_locked - &session.amount_settled;
        session.status = SessionStatus::Closed as u8;

        self.sessions(&channel_id).set(&session);

        if refund > 0u64 {
            self.send_tokens(&session.employer, &session.token_identifier, &refund);
        }

        self.close_session_event(&channel_id, &session.amount_settled, &refund);
    }

    // Storage

    #[view(getSession)]
    #[storage_mapper("sessions")]
    fn sessions(&self, channel_id: &ManagedBuffer) -> SingleValueMapper<SessionData<Self::Api>>;

    #[storage_mapper("last_channel_nonce")]
    fn last_channel_nonce(&self, employer: &ManagedAddress) -> SingleValueMapper<u64>;

    #[storage_mapper("last_id")]
    fn last_id(&self) -> SingleValueMapper<ManagedBuffer>;

    // Events

    #[event("open_session")]
    fn open_session_event(
        &self,
        #[indexed] channel_id: &ManagedBuffer,
        #[indexed] employer: &ManagedAddress,
        #[indexed] receiver: &ManagedAddress,
        #[indexed] token_id: &EgldOrEsdtTokenIdentifier,
        amount: &BigUint,
    );

    #[event("settle_session")]
    fn settle_session_event(
        &self,
        #[indexed] channel_id: &ManagedBuffer,
        #[indexed] nonce: u64,
        amount: &BigUint,
    );

    #[event("close_session")]
    fn close_session_event(
        &self,
        #[indexed] channel_id: &ManagedBuffer,
        #[indexed] amount: &BigUint,
        refund_amount: &BigUint,
    );

    // Helpers

    fn send_tokens(&self, to: &ManagedAddress, token_id: &EgldOrEsdtTokenIdentifier, amount: &BigUint) {
        if token_id.is_egld() {
            self.send().direct_egld(to, amount);
        } else {
            self.send().direct_esdt(to, &token_id.clone().unwrap_esdt(), 0, amount);
        }
    }

    fn verify_voucher(&self, channel_id: &ManagedBuffer, amount: &BigUint, nonce: u64, signature: &ManagedBuffer, employer: &ManagedAddress) {
        let mut message = ManagedBuffer::new();
        message.append_bytes(b"mpp-session-v1");
        message.append(self.blockchain().get_sc_address().as_managed_buffer());
        message.append(channel_id);
        
        let amount_buf = amount.to_bytes_be_buffer();
        message.append(&amount_buf);
        
        let nonce_bytes = nonce.to_be_bytes();
        message.append_bytes(&nonce_bytes[..]);

        let hash = self.crypto().keccak256(&message);
        
        self.crypto().verify_ed25519(
            employer.as_managed_buffer(),
            hash.as_managed_buffer(),
            signature
        );
    }
}

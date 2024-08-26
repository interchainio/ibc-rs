pub mod send;
// TODO: ibc-go v9 deprecated the token field in MsgTransfer.
// Keep the field to be compatible with previous versions until all
// previous versions get to end-of-life.
#[allow(deprecated)]
pub mod transfer;

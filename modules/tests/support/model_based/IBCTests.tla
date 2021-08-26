------------------------------ MODULE IBCTests --------------------------------

EXTENDS IBC

\* ICS02CreateClient tests
ICS02CreateOKTest ==
    /\ actionOutcome = "Ics02CreateOk"

\* ICS02UpdateClient tests
ICS02UpdateOKTest ==
    /\ actionOutcome = "Ics02UpdateOk"

ICS02ClientNotFoundTest ==
    /\ actionOutcome = "Ics02ClientNotFound"

ICS02HeaderVerificationFailureTest ==
    /\ actionOutcome = "Ics02HeaderVerificationFailure"

\* ICS07UpgradeClient tests
ICS07UpgradeOkTest ==
    /\ actionOutcome = "Ics07UpgradeOk"

ICS07ClientNotFoundTest ==
    /\ actionOutcome = "Ics07ClientNotFound"

ICS07HeaderVerificationFailureTest ==
    /\ actionOutcome = "Ics07HeaderVerificationFailure"


\* ICS03ConnectionOpenInit tests
ICS03ConnectionOpenInitOKTest ==
    /\ actionOutcome = "Ics03ConnectionOpenInitOk"

ICS03MissingClientTest ==
    /\ actionOutcome = "Ics03MissingClient"

\* ICS03ConnectionOpenTry tests
ICS03ConnectionOpenTryOKTest ==
    /\ actionOutcome = "Ics03ConnectionOpenTryOk"

ICS03InvalidConsensusHeightTest ==
    /\ actionOutcome = "Ics03InvalidConsensusHeight"

ICS03ConnectionNotFoundTest ==
    /\ actionOutcome = "Ics03ConnectionNotFound"

ICS03ConnectionMismatchTest ==
    /\ actionOutcome = "Ics03ConnectionMismatch"

ICS03MissingClientConsensusStateTest ==
    /\ actionOutcome = "Ics03MissingClientConsensusState"

\* TODO: the following test should fail but doesn't because proofs are not yet
\*       verified in the implementation
\* ICS03InvalidProofTest ==
\*     /\ actionOutcome = "Ics03InvalidProof"

\* ICS03ConnectionOpenAck tests
ICS03ConnectionOpenAckOKTest ==
    /\ actionOutcome = "Ics03ConnectionOpenAckOk"

\* ICS03ConnectionOpenConfirm tests
ICS03ConnectionOpenConfirmOKTest ==
    /\ actionOutcome = "Ics03ConnectionOpenConfirmOk"
===============================================================================

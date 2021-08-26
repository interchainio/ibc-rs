--------------------------------- MODULE IBC ----------------------------------

EXTENDS ICS02, ICS03

\* ids of existing chains
CONSTANT ChainIds
\* max height which chains can reach
CONSTANT MaxRevisionHeight
ASSUME MaxRevisionHeight >= 0
\* max revision which chains can reach
CONSTANT MaxRevisionNumber
ASSUME MaxRevisionNumber >= 0
\* max number of client to be created per chain
CONSTANT MaxClientsPerChain
ASSUME MaxClientsPerChain >= 0
\* max number of connections to be created per chain
CONSTANT MaxConnectionsPerChain
ASSUME MaxConnectionsPerChain >= 0

\* mapping from chain id to its data
VARIABLE chains
\* last action performed
VARIABLE action
\* string with the outcome of the last operation
VARIABLE actionOutcome
vars == <<chains, action, actionOutcome>>

\* set of possible height tuples
Heights == [ revision_number: (1..MaxRevisionNumber), revision_height: (1..MaxRevisionHeight) ]
MaxHeight == [ revision_number |-> MaxRevisionNumber, revision_height |-> MaxRevisionHeight ]
\* set of possible client identifiers
ClientIds == 0..(MaxClientsPerChain - 1)
\* set of possible connection identifiers
ConnectionIds == 0..(MaxConnectionsPerChain - 1)
\* set of possible connection states
ConnectionStates == {
    "Uninitialized",
    "Init",
    "TryOpen",
    "Open"
}

\* set of possible actions
NoneActions == [
    type: {"None"}
] <: {ActionType}

CreateClientActions == [
    type: {"Ics02CreateClient"},
    chainId: ChainIds,
    \* `clientState` contains simply a height
    clientState: Heights,
    \* `consensusState` contains simply a height
    consensusState: Heights
] <: {ActionType}
UpdateClientActions == [
    type: {"Ics02UpdateClient"},
    chainId: ChainIds,
    clientId: ClientIds,
    \* `header` contains simply a height
    header: Heights
] <: {ActionType}
UpgradeClientActions == [
    type: {"Ics07UpgradeClient"},
    chainId: ChainIds,
    clientId: ClientIds,
    \* `header` contains simply a height
    header: Heights
] <: {ActionType}
ClientActions ==
    CreateClientActions \union
    UpdateClientActions \union
    UpgradeClientActions

ConnectionOpenInitActions == [
    type: {"Ics03ConnectionOpenInit"},
    chainId: ChainIds,
    clientId: ClientIds,
    counterpartyChainId: ChainIds,
    counterpartyClientId: ClientIds
] <: {ActionType}
ConnectionOpenTryActions == [
    type: {"Ics03ConnectionOpenTry"},
    chainId: ChainIds,
    clientId: ClientIds,
    \* `previousConnectionId` can be none
    previousConnectionId: ConnectionIds \union {ConnectionIdNone},
    \* `clientState` contains simply a height
    clientState: Heights,
    counterpartyChainId: ChainIds,
    counterpartyClientId: ClientIds,
    counterpartyConnectionId: ConnectionIds
] <: {ActionType}
ConnectionOpenAckActions == [
    type: {"Ics03ConnectionOpenAck"},
    chainId: ChainIds,
    connectionId: ConnectionIds,
    \* `clientState` contains simply a height
    clientState: Heights,
    counterpartyChainId: ChainIds,
    counterpartyConnectionId: ConnectionIds
] <: {ActionType}
ConnectionOpenConfirmActions == [
    type: {"Ics03ConnectionOpenConfirm"},
    chainId: ChainIds,
    connectionId: ConnectionIds,
    \* `clientState` contains simply a height
    clientState: Heights,
    counterpartyChainId: ChainIds,
    counterpartyConnectionId: ConnectionIds
] <: {ActionType}
ConnectionActions ==
    ConnectionOpenInitActions \union
    ConnectionOpenTryActions \union
    ConnectionOpenAckActions \union
    ConnectionOpenConfirmActions

Actions ==
    NoneActions \union
    ClientActions \union 
    ConnectionActions

\* set of possible action outcomes
ActionOutcomes == {
    "None",
    "ModelError",
    \* ICS02_CreateClient outcomes:
    "Ics02CreateOk",
    \* ICS02_UpdateClient outcomes:
    "Ics02UpdateOk",
    "Ics02ClientNotFound",
    "Ics02HeaderVerificationFailure",
    \* ICS07_UpgradeClient outcomes:
    "Ics07UpgradeOk",
    "Ics07ClientNotFound",
    "Ics07HeaderVerificationFailure",
    \* ICS03_ConnectionOpenInit outcomes:
    "Ics03ConnectionOpenInitOk",
    "Ics03MissingClient",
    \* ICS03_ConnectionOpenTry outcomes:
    "Ics03ConnectionOpenTryOk",
    "Ics03InvalidConsensusHeight",
    "Ics03ConnectionNotFound",
    "Ics03ConnectionMismatch",
    "Ics03MissingClientConsensusState",
    "Ics03InvalidProof",
    \* ICS03_ConnectionOpenAck outcomes:
    "Ics03ConnectionOpenAckOk",
    \* ICS03_ConnectionOpenConfirm outcomes:
    "Ics03ConnectionOpenConfirmOk"
}
\* TODO: the current generation of tests cannot distinguish between a
\*       "Ics03ConnectionMismatch" generated in conn open try, one generated
\*       in conn open ack, or one genereted in conn open confirm;
\*       (there are other cases like "Ics03InvalidProof")
\*       we can solve this with in a variable 'history', like in the light
\*       client tests.

\* data kept per client
Client == [
    heights: SUBSET Heights
]
\* mapping from client identifier to its height
Clients == [
    ClientIds -> Client
]
\* data kept per connection
Connection == [
    state: ConnectionStates,
    \* `chainId` is not strictly necessary but it's kept for consistency
    chainId: ChainIds \union {ChainIdNone},
    clientId: ClientIds \union {ClientIdNone},
    connectionId: ConnectionIds \union {ConnectionIdNone},
    counterpartyChainId: ChainIds \union {ChainIdNone},
    counterpartyClientId: ClientIds \union {ClientIdNone},
    counterpartyConnectionId: ConnectionIds \union {ConnectionIdNone}
]
\* mapping from connection identifier to its data
Connections == [
    ConnectionIds -> Connection
]
\* data kept per chain
Chain == [
    height: Heights,
    clients: Clients,
    clientIdCounter: 0..MaxClientsPerChain,
    connections: Connections,
    connectionIdCounter: 0..MaxConnectionsPerChain,
    connectionProofs: SUBSET ConnectionActions
]
\* mapping from chain identifier to its data
Chains == [
    ChainIds -> Chain
]

(***************************** Specification *********************************)

\* update block height if outcome was ok
UpdateRevisionHeight(height, result, okOutcome) ==
    IF result.outcome = okOutcome THEN
        \* <<height[1], height[2] + 1>>
        [ revision_number |-> height.revision_number, revision_height |-> height.revision_height + 1 ]
    ELSE
        height

\* update revision height if outcome was ok
UpdateRevisionNumber(height, result, okOutcome) ==
    IF result.outcome = okOutcome THEN
        [ revision_number |-> height.revision_number + 1, revision_height |-> height.revision_height ]
    ELSE
        height


\* update connection proofs if outcome was ok
UpdateConnectionProofs(connectionProofs, result, okOutcome) ==
    IF result.outcome = okOutcome THEN
        connectionProofs \union {result.action}
    ELSE
        connectionProofs

CreateClient(chainId, height) ==
    LET chain == chains[chainId] IN
    LET result == ICS02_CreateClient(chain, chainId, height) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics02CreateOk"),
        !.clients = result.clients,
        !.clientIdCounter = result.clientIdCounter
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT ![chainId] = updatedChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

UpdateClient(chainId, clientId, height) ==
    LET chain == chains[chainId] IN
    LET result == ICS02_UpdateClient(chain, chainId, clientId, height) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics02UpdateOk"),
        !.clients = result.clients
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT ![chainId] = updatedChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

UpgradeClient(chainId, clientId, height) ==
    LET chain == chains[chainId] IN
    LET result == ICS07_UpgradeClient(chain, chainId, clientId, height) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics07UpgradeOk"),
        !.clients = result.clients
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT ![chainId] = updatedChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

ConnectionOpenInit(
    chainId,
    clientId,
    counterpartyChainId,
    counterpartyClientId
) ==
    LET chain == chains[chainId] IN
    LET result == ICS03_ConnectionOpenInit(
        chain,
        chainId,
        clientId,
        counterpartyChainId,
        counterpartyClientId
    ) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics03ConnectionOpenInitOk"),
        !.connections = result.connections,
        !.connectionIdCounter = result.connectionIdCounter
    ] IN
    \* update the counterparty chain with a proof
    LET counterpartyChain == chains[counterpartyChainId] IN
    LET updatedCounterpartyChain == [counterpartyChain EXCEPT
        !.connectionProofs = UpdateConnectionProofs(@, result, "Ics03ConnectionOpenInitOk")
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT
        ![chainId] = updatedChain,
        ![counterpartyChainId] = updatedCounterpartyChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

ConnectionOpenTry(
    chainId,
    clientId,
    previousConnectionId,
    height,
    counterpartyChainId,
    counterpartyClientId,
    counterpartyConnectionId
) ==
    LET chain == chains[chainId] IN
    LET result == ICS03_ConnectionOpenTry(
        chain,
        chainId,
        clientId,
        previousConnectionId,
        height,
        counterpartyChainId,
        counterpartyClientId,
        counterpartyConnectionId
    ) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics03ConnectionOpenTryOk"),
        !.connections = result.connections,
        !.connectionIdCounter = result.connectionIdCounter
    ] IN
    \* update the counterparty chain with a proof
    LET counterpartyChain == chains[counterpartyChainId] IN
    LET updatedCounterpartyChain == [counterpartyChain EXCEPT
        !.connectionProofs = UpdateConnectionProofs(@, result, "Ics03ConnectionOpenTryOk")
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT
        ![chainId] = updatedChain,
        ![counterpartyChainId] = updatedCounterpartyChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

ConnectionOpenAck(
    chainId,
    connectionId,
    height,
    counterpartyChainId,
    counterpartyConnectionId
) ==
    LET chain == chains[chainId] IN
    LET result == ICS03_ConnectionOpenAck(
        chain,
        chainId,
        connectionId,
        height,
        counterpartyChainId,
        counterpartyConnectionId
    ) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics03ConnectionOpenAckOk"),
        !.connections = result.connections
    ] IN
    \* update the counterparty chain with a proof
    LET counterpartyChain == chains[counterpartyChainId] IN
    LET updatedCounterpartyChain == [counterpartyChain EXCEPT
        !.connectionProofs = UpdateConnectionProofs(@, result, "Ics03ConnectionOpenAckOk")
    ] IN
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT
        ![chainId] = updatedChain,
        ![counterpartyChainId] = updatedCounterpartyChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

ConnectionOpenConfirm(
    chainId,
    connectionId,
    height,
    counterpartyChainId,
    counterpartyConnectionId
) ==
    LET chain == chains[chainId] IN
    LET result == ICS03_ConnectionOpenConfirm(
        chain,
        chainId,
        connectionId,
        height,
        counterpartyChainId,
        counterpartyConnectionId
    ) IN
    \* update the chain
    LET updatedChain == [chain EXCEPT
        !.height = UpdateRevisionHeight(@, result, "Ics03ConnectionOpenConfirmOk"),
        !.connections = result.connections
    ] IN
    \* no need to update the counterparty chain with a proof (as in the other
    \* connection open handlers)
    \* update `chains`, set the `action` and its `actionOutcome`
    /\ chains' = [chains EXCEPT ![chainId] = updatedChain]
    /\ action' = result.action
    /\ actionOutcome' = result.outcome

CreateClientAction(chainId) ==
    \* select a height for the client to be created at
    \E height \in Heights:
        \* only create client if the model constant `MaxClientsPerChain` allows
        \* it
        LET allowed == chains[chainId].clientIdCounter < MaxClientsPerChain IN
        IF allowed THEN
            CreateClient(chainId, height)
        ELSE
            UNCHANGED vars

UpdateClientAction(chainId) ==
    \* select a client to be updated (which may not exist)
    \E clientId \in ClientIds: 
    \* select a height for the client to be updated
    \* We only use heights at the same revision number to save on state space
    \E height \in {height \in Heights: height.revision_number = chains[chainId].height.revision_number}:
        UpdateClient(chainId, clientId, height)
        
UpgradeClientAction(chainId) ==
    \* select a client to be upgraded (which may not exist)
    \E clientId \in ClientIds:
    \* select a height for the client to be upgraded
    \* We only try to upgrade to heights with a height of one to save on state space
    \E height \in {height \in Heights: height.revision_height = 1}:
        UpgradeClient(chainId, clientId, height)

ConnectionOpenInitAction(chainId) ==
    \* select a client id
    \E clientId \in ClientIds:
    \* select a counterparty chain id
    \E counterpartyChainId \in ChainIds:
    \* select a counterparty client id
    \E counterpartyClientId \in ClientIds:
        \* only create connection if the model constant `MaxConnectionsPerChain`
        \* allows it
        LET allowed ==
            chains[chainId].connectionIdCounter < MaxConnectionsPerChain IN
        IF chainId /= counterpartyChainId /\ allowed THEN
            ConnectionOpenInit(
                chainId,
                clientId,
                counterpartyChainId,
                counterpartyClientId
            )
        ELSE
            UNCHANGED vars

ConnectionOpenTryAction(chainId) ==
    \* select a client id
    \E clientId \in ClientIds:
    \* select a previous connection id (which can be none)
    \E previousConnectionId \in ConnectionIds \union {ConnectionIdNone}:
    \* select a claimed height for the client
    \* Only use heights whose revision number is 1 (this covers updates) OR whose revision height <= 2 (this allows for an upgrade and an update, but no updates after that)
    \E height \in {height \in Heights: height.revision_number <= 2 /\ height.revision_height <= 2}:
    \* select a counterparty chain id
    \E counterpartyChainId \in ChainIds:
    \* select a counterparty client id
    \E counterpartyClientId \in ClientIds:
    \* select a counterparty connection id
    \E counterpartyConnectionId \in ConnectionIds:
        \* only perform action if there was a previous connection or if the
        \* model constant `MaxConnectionsPerChain` allows that a new connection
        \* is created
        LET allowed ==
            \/ previousConnectionId /= ConnectionIdNone
            \/ chains[chainId].connectionIdCounter < MaxConnectionsPerChain IN
        IF chainId /= counterpartyChainId /\ allowed THEN
            ConnectionOpenTry(
                chainId,
                clientId,
                previousConnectionId,
                height,
                counterpartyChainId,
                counterpartyClientId,
                counterpartyConnectionId
            )
        ELSE
            UNCHANGED vars

ConnectionOpenAckAction(chainId) ==
    \* select a connection id
    \E connectionId \in ConnectionIds:
    \* select a claimed height for the client
    \* Only use heights whose revision number is 1 (this covers updates) OR whose revision height <= 2 (this allows for an upgrade but no updates after that)
    \E height \in {height \in Heights: height.revision_number <= 2 /\ height.revision_height <= 2}:
    \* select a counterparty chain id
    \E counterpartyChainId \in ChainIds:
    \* select a counterparty connection id
    \E counterpartyConnectionId \in ConnectionIds:
        IF chainId /= counterpartyChainId THEN
            ConnectionOpenAck(
                chainId,
                connectionId,
                height,
                counterpartyChainId,
                counterpartyConnectionId
            )
        ELSE
            UNCHANGED vars

ConnectionOpenConfirmAction(chainId) ==
    \* select a connection id
    \E connectionId \in ConnectionIds:
    \* select a claimed height for the client
    \* Only use heights whose revision number is 1 (this covers updates) OR whose revision height <= 2 (this allows for an upgrade but no updates after that)
    \E height \in {height \in Heights: height.revision_number <= 2 /\ height.revision_height <= 2}:
    \* select a counterparty chain id
    \E counterpartyChainId \in ChainIds:
    \* select a counterparty connection id
    \E counterpartyConnectionId \in ConnectionIds:
        IF chainId /= counterpartyChainId THEN
            ConnectionOpenConfirm(
                chainId,
                connectionId,
                height,
                counterpartyChainId,
                counterpartyConnectionId
            )
        ELSE
            UNCHANGED vars

Init ==
    \* create a client and a connection with none values
    LET clientNone == [
        heights |-> AsSetInt({})
    ] IN
    LET connectionNone == [
        state |-> "Uninitialized",
        chainId |-> ChainIdNone,
        clientId |-> ClientIdNone,
        connectionId |-> ConnectionIdNone,
        counterpartyChainId |-> ChainIdNone,
        counterpartyClientId |-> ClientIdNone,
        counterpartyConnectionId |-> ConnectionIdNone
    ] IN
    \* create an empty chain
    LET emptyChain == [
        height |-> [ revision_number |-> 1, revision_height |-> 1 ],
        clients |-> [clientId \in ClientIds |-> clientNone],
        clientIdCounter |-> 0,
        connections |-> [connectionId \in ConnectionIds |-> connectionNone],
        connectionIdCounter |-> 0,
        connectionProofs |-> AsSetAction({})
    ] IN
    /\ chains = [chainId \in ChainIds |-> emptyChain]
    /\ action = AsAction([type |-> "None"])
    /\ actionOutcome = "None"

Next ==
    \* select a chain id
    \E chainId \in ChainIds:
        \* perform action on chain if the model constant `MaxChainHeight` allows
        \* it
        \* The line below checks if chains[chainId].height < MaxHeight
        IF chains[chainId].height.revision_number < MaxHeight.revision_number /\ chains[chainId].height.revision_height < MaxHeight.revision_height THEN
            \/ CreateClientAction(chainId)
            \/ UpdateClientAction(chainId)
            \/ UpgradeClientAction(chainId)
            \/ ConnectionOpenInitAction(chainId)
            \/ ConnectionOpenTryAction(chainId)
            \/ ConnectionOpenAckAction(chainId)
            \/ ConnectionOpenConfirmAction(chainId)
            \/ UNCHANGED vars
        ELSE
            \/ UNCHANGED vars

(******************************** Invariants *********************************)

TypeOK ==
    /\ chains \in Chains
    /\ action \in Actions
    /\ actionOutcome \in ActionOutcomes

\* the model never erros
ModelNeverErrors ==
    actionOutcome /= "ModelError"

===============================================================================

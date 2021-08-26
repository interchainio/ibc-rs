------------------------------ MODULE ICS03 -----------------------------------

EXTENDS ICS02

\* retrieves `connectionId`'s data
ICS03_GetConnection(connections, connectionId) ==
    connections[connectionId]

\* check if `connectionId` exists
ICS03_ConnectionExists(connections, connectionId) ==
    ICS03_GetConnection(connections, connectionId).state /= "Uninitialized"

\* update `connectionId`'s data
ICS03_SetConnection(connections, connectionId, connection) ==
    [connections EXCEPT ![connectionId] = connection]

ICS03_ConnectionOpenInit(
    chain,
    chainId,
    clientId,
    counterpartyChainId,
    counterpartyClientId
) ==
    LET action_ == AsAction([
        type |-> "Ics03ConnectionOpenInit",
        chainId |-> chainId,
        clientId |-> clientId,
        counterpartyChainId |-> counterpartyChainId,
        counterpartyClientId |-> counterpartyClientId
    ]) IN
    \* check if the client exists
    IF ~ICS02_ClientExists(chain.clients, clientId) THEN
        \* if the client does not exist, then set an error outcome
        [
            connections |-> chain.connections,
            connectionIdCounter |-> chain.connectionIdCounter,
            action |-> action_,
            outcome |-> "Ics03MissingClient"
        ]
    ELSE
        \* if the client exists,
        \* then check if the connection exists (it shouldn't)
        IF ICS03_ConnectionExists(chain.connections, chain.connectionIdCounter) THEN
            \* if the connection to be created already exists,
            \* then there's an error in the model
            [
                connections |-> chain.connections,
                connectionIdCounter |-> chain.connectionIdCounter,
                action |-> action_,
                outcome |-> "ModelError"
            ]
        ELSE
            \* if it doesn't, create it
            LET connection == [
                state |-> "Init",
                chainId |-> chainId,
                clientId |-> clientId,
                \* generate a new connection identifier
                connectionId |-> chain.connectionIdCounter,
                counterpartyChainId |-> counterpartyChainId,
                counterpartyClientId |-> counterpartyClientId,
                counterpartyConnectionId |-> ConnectionIdNone
            ] IN
            \* return result with updated state
            [
                connections |-> ICS03_SetConnection(
                    chain.connections,
                    chain.connectionIdCounter,
                    connection
                ),
                connectionIdCounter |-> chain.connectionIdCounter + 1,
                action |-> action_,
                outcome |-> "Ics03ConnectionOpenInitOk"
            ]

ICS03_ConnectionOpenTry(
    chain,
    chainId,
    clientId,
    previousConnectionId,
    height,
    counterpartyChainId,
    counterpartyClientId,
    counterpartyConnectionId
) ==
    LET action_ == AsAction([
        type |-> "Ics03ConnectionOpenTry",
        chainId |-> chainId,
        clientId |-> clientId,
        previousConnectionId |-> previousConnectionId,
        clientState |-> height,
        counterpartyChainId |-> counterpartyChainId,
        counterpartyClientId |-> counterpartyClientId,
        counterpartyConnectionId |-> counterpartyConnectionId
    ]) IN
    \* check if client's claimed height is higher than the chain's height
    IF HeightGT(height, chain.height) THEN
        \* if client's height is too advanced, then set an error outcome
        [
            connections |-> chain.connections,
            connectionIdCounter |-> chain.connectionIdCounter,
            action |-> action_,
            outcome |-> "Ics03InvalidConsensusHeight"
        ]
        \* TODO: add `chain_max_history_size` to the model to be able to also
        \*       return a `ICS03StaleConsensusHeight` error outcome
    ELSE
        \* check if there's a `previousConnectionId`. this situation can happen
        \* where there are two concurrent open init's establishing a connection
        \* between the same two chains, say chainA and chainB; then, when chainB
        \* sees the open init from chainA, instead of creating a new connection
        \* identifier, it can reuse the identifier created by its own open init.
        IF previousConnectionId /= ConnectionIdNone THEN
            \* if so, check if the connection exists
            IF ~ICS03_ConnectionExists(chain.connections, previousConnectionId) THEN
                \* if the connection does not exist, then set an error outcome
                [
                    connections |-> chain.connections,
                    connectionIdCounter |-> chain.connectionIdCounter,
                    action |-> action_,
                    outcome |-> "Ics03ConnectionNotFound"
                ]
            ELSE
                \* if the connection exists, verify that is matches the the
                \* parameters provided
                LET connection == ICS03_GetConnection(
                    chain.connections,
                    previousConnectionId
                ) IN
                LET validConnection ==
                    /\ connection.state = "Init"
                    /\ connection.clientId = clientId
                    /\ connection.counterpartyClientId = counterpartyClientId
                    /\ connection.counterpartyConnectionId = counterpartyConnectionId IN
                IF ~validConnection THEN
                    \* if the existing connection does not match, then set an
                    \* error outcome
                    [
                        connections |-> chain.connections,
                        connectionIdCounter |-> chain.connectionIdCounter,
                        action |-> action_,
                        outcome |-> "Ics03ConnectionMismatch"
                    ]
                ELSE
                    \* verification passed; update the connection state to
                    \* "TryOpen"
                    LET updatedConnection == [connection EXCEPT
                        !.state = "TryOpen"
                    ] IN
                    \* return result with updated state
                    [
                        connections |-> ICS03_SetConnection(
                            chain.connections,
                            previousConnectionId,
                            updatedConnection
                        ),
                        \* as the connection identifier has already been
                        \* created, here we do not update the
                        \* `connectionIdCounter`
                        connectionIdCounter |-> chain.connectionIdCounter,
                        action |-> action_,
                        outcome |-> "Ics03ConnectionOpenTryOk"
                    ]
        ELSE
            \* in this case, `previousConnectionId` was not set; check if the
            \* client exists
            IF ~ICS02_ClientExists(chain.clients, clientId) THEN
                \* if the client does not exist, then set an error outcome
                [
                    connections |-> chain.connections,
                    connectionIdCounter |-> chain.connectionIdCounter,
                    action |-> action_,
                    outcome |-> "Ics03MissingClient"
                ]
            ELSE
                \* check if the client has a consensus state with this height
                LET client == ICS02_GetClient(chain.clients, clientId) IN
                LET consensusStateExists == height \in client.heights IN
                IF ~consensusStateExists THEN
                    \* if the client does have a consensus state with this
                    \* height, then set an error outcome
                    [
                        connections |-> chain.connections,
                        connectionIdCounter |-> chain.connectionIdCounter,
                        action |-> action_,
                        outcome |-> "Ics03MissingClientConsensusState"
                    ]
                ELSE
                    \* check if there was an open init at the remote chain
                    LET openInitProofs == {
                        proof \in chain.connectionProofs :
                            /\ proof.type = "Ics03ConnectionOpenInit"
                            /\ proof.chainId = counterpartyChainId
                            /\ proof.clientId = counterpartyClientId
                            /\ proof.counterpartyChainId = chainId
                            /\ proof.counterpartyClientId = clientId
                    } IN
                    LET proofExists == Cardinality(openInitProofs) > 0 IN
                    IF ~proofExists THEN
                        \* if there wasn't an open init at the remote chain,
                        \* then set an error outcome
                        [
                            connections |-> chain.connections,
                            connectionIdCounter |-> chain.connectionIdCounter,
                            action |-> action_,
                            outcome |-> "Ics03InvalidProof"
                        ]
                    ELSE
                        \* verification passed; create connection
                        LET connection == [
                            state |-> "TryOpen",
                            chainId |-> chainId,
                            clientId |-> clientId,
                            \* generate a new connection identifier
                            connectionId |-> chain.connectionIdCounter,
                            counterpartyChainId |-> counterpartyChainId,
                            counterpartyClientId |-> counterpartyClientId,
                            counterpartyConnectionId |-> counterpartyConnectionId
                        ] IN
                        \* return result with updated state
                        [
                            connections |-> ICS03_SetConnection(
                                chain.connections,
                                chain.connectionIdCounter,
                                connection
                            ),
                            \* since a new connection identifier has been
                            \* created, here we update the `connectionIdCounter`
                            connectionIdCounter |-> chain.connectionIdCounter + 1,
                            action |-> action_,
                            outcome |-> "Ics03ConnectionOpenTryOk"
                        ]

ICS03_ConnectionOpenAck(
    chain,
    chainId,
    connectionId,
    height,
    counterpartyChainId,
    counterpartyConnectionId
) ==
    LET action_ == AsAction([
        type |-> "Ics03ConnectionOpenAck",
        chainId |-> chainId,
        connectionId |-> connectionId,
        clientState |-> height,
        counterpartyChainId |-> counterpartyChainId,
        counterpartyConnectionId |-> counterpartyConnectionId
    ]) IN
    LET clients == chain.clients IN
    LET connections == chain.connections IN
    LET connectionProofs == chain.connectionProofs IN
    \* check if client's claimed height is higher than the chain's height
    IF HeightGT(height, chain.height) THEN
        \* if client's height is too advanced, then set an error outcome
        [
            connections |-> connections,
            action |-> action_,
            outcome |-> "Ics03InvalidConsensusHeight"
        ]
        \* TODO: add `chain_max_history_size` to the model to be able to also
        \*       return a `ICS03StaleConsensusHeight` error outcome
    ELSE
        \* check if the connection exists
        IF ~ICS03_ConnectionExists(connections, connectionId) THEN
            \* if the connection does not exist, then set an error outcome
            [
                connections |-> connections,
                action |-> action_,
                outcome |-> "Ics03ConnectionNotFound"
            ]
        ELSE
            \* if the connection exists, verify that is either Init or TryOpen;
            \* also check that the counterparty connection id matches
            LET connection == ICS03_GetConnection(connections, connectionId) IN
            LET validConnection ==
                /\ connection.state \in {"Init", "TryOpen"} 
                \* TODO: the implementation is not checking the following;
                \*       should it?
                /\ connection.counterpartyChainId = counterpartyChainId IN
            IF ~validConnection THEN
                \* if the existing connection does not match, then set an
                \* error outcome
                [
                    connections |-> connections,
                    action |-> action_,
                    outcome |-> "Ics03ConnectionMismatch"
                ]
            ELSE
                \* check if the client has a consensus state with this height
                LET client == ICS02_GetClient(clients, connection.clientId) IN
                LET consensusStateExists == height \in client.heights IN
                IF ~consensusStateExists THEN
                    \* if the client does have a consensus state with this
                    \* height, then set an error outcome
                    [
                        connections |-> connections,
                        action |-> action_,
                        outcome |-> "Ics03MissingClientConsensusState"
                    ]
                ELSE
                    \* check if there was an open try at the remote chain
                    LET openTryProofs == {
                        proof \in chain.connectionProofs :
                            /\ proof.type = "Ics03ConnectionOpenTry"
                            /\ proof.chainId = connection.counterpartyChainId
                            /\ proof.clientId = connection.counterpartyClientId
                            /\ proof.counterpartyChainId = connection.chainId
                            /\ proof.counterpartyClientId = connection.clientId
                            /\ proof.counterpartyConnectionId = connectionId
                    } IN
                    LET proofExists == Cardinality(openTryProofs) > 0 IN
                    IF ~proofExists THEN
                        \* if there wasn't an open try at the remote chain,
                        \* then set an error outcome
                        [
                            connections |-> chain.connections,
                            action |-> action_,
                            outcome |-> "Ics03InvalidProof"
                        ]
                    ELSE
                        \* verification passed; update the connection state to
                        \* "Open"
                        LET updatedConnection == [connection EXCEPT
                            !.state = "Open"
                        ] IN
                        \* return result with updated state
                        [
                            connections |-> ICS03_SetConnection(
                                connections,
                                connectionId,
                                updatedConnection
                            ),
                            action |-> action_,
                            outcome |-> "Ics03ConnectionOpenAckOk"
                        ]

ICS03_ConnectionOpenConfirm(
    chain,
    chainId,
    connectionId,
    height,
    counterpartyChainId,
    counterpartyConnectionId
) ==
    LET action_ == AsAction([
        type |-> "Ics03ConnectionOpenConfirm",
        chainId |-> chainId,
        connectionId |-> connectionId,
        clientState |-> height,
        counterpartyChainId |-> counterpartyChainId,
        counterpartyConnectionId |-> counterpartyConnectionId
    ]) IN
    LET clients == chain.clients IN
    LET connections == chain.connections IN
    LET connectionProofs == chain.connectionProofs IN
    \* check if the connection exists
    IF ~ICS03_ConnectionExists(connections, connectionId) THEN
        \* if the connection does not exist, then set an error outcome
        [
            connections |-> connections,
            action |-> action_,
            outcome |-> "Ics03ConnectionNotFound"
        ]
    ELSE
        \* if the connection exists, verify that is either Init or TryOpen;
        \* also check that the counterparty connection id matches
        LET connection == ICS03_GetConnection(connections, connectionId) IN
        LET validConnection == connection.state = "TryOpen" IN
        IF ~validConnection THEN
            \* if the existing connection does not match, then set an
            \* error outcome
            [
                connections |-> connections,
                action |-> action_,
                outcome |-> "Ics03ConnectionMismatch"
            ]
        ELSE
            \* check if the client has a consensus state with this height
            LET client == ICS02_GetClient(clients, connection.clientId) IN
            LET consensusStateExists == height \in client.heights IN
            IF ~consensusStateExists THEN
                \* if the client does have a consensus state with this
                \* height, then set an error outcome
                [
                    connections |-> connections,
                    action |-> action_,
                    outcome |-> "Ics03MissingClientConsensusState"
                ]
            ELSE
                \* check if there was an open ack at the remote chain
                LET openAckProofs == {
                    proof \in chain.connectionProofs :
                        /\ proof.type = "Ics03ConnectionOpenAck"
                        /\ proof.chainId = connection.counterpartyChainId
                        /\ proof.connectionId = connection.counterpartyConnectionId
                        /\ proof.counterpartyChainId = connection.chainId
                        /\ proof.counterpartyConnectionId = connectionId
                } IN
                LET proofExists == Cardinality(openAckProofs) > 0 IN
                IF ~proofExists THEN
                    \* if there wasn't an open ack at the remote chain,
                    \* then set an error outcome
                    [
                        connections |-> chain.connections,
                        action |-> action_,
                        outcome |-> "Ics03InvalidProof"
                    ]
                ELSE
                    \* verification passed; update the connection state to
                    \* "Open"
                    LET updatedConnection == [connection EXCEPT
                        !.state = "Open"
                    ] IN
                    \* return result with updated state
                    [
                        connections |-> ICS03_SetConnection(
                            connections,
                            connectionId,
                            updatedConnection
                        ),
                        action |-> action_,
                        outcome |-> "Ics03ConnectionOpenConfirmOk"
                    ]

===============================================================================

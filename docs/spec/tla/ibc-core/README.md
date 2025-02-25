# TLA+ specification of the IBC Core protocols

A TLA+ specification of the IBC Core protocols ([ICS02](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-002-client-semantics), [ICS03](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-003-connection-semantics), [ICS04](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics), [ICS18](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-018-relayer-algorithms)).
In particular, the main module is [IBCCore.tla](IBCCore.tla) and models a 
system consisting of two chains and two relayers. 
The model allows to express concurrency aspects of a system with multiple (correct) relayers.
The specification is written in a modular way, in order to facilitate future 
formal verification of properties and invariants in an adversarial setting.

## Modules

The specification has the following modules: 
  - `IBCCore.tla` (the main module)
  - `ICS18Relayer.tla` 
  - `Chain.tla`
  - `ICS02ClientHandlers.tla`
  - `ICS03ConnectionHandlers.tla`
  - `ICS04ChannelHandlers.tla`
  - `ICS04PacketHandlers.tla`
  - `IBCCoreDefinitions.tla`

### [`ICS18Relayer.tla`](ICS18Relayer.tla)
A relayer relays datagrams between the two chains. Its transition relation is defined by the formula:
```tla
Next ==
    \/ UpdateClient
    \/ CreateDatagrams
    \/ UNCHANGED vars    
```
where `UpdateClient` and `CreateDatagrams` are scheduled non-deterministically. 
`UpdateClient` picks a light client on the relayer for some chain and updates it. `CreateDatagrams` picks a direction (a pair of source and destination chain) and 
creates client, connection, channel, and packet datagrams (i.e., it captures the 
logic of [`pendingDatagrams()`](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-018-relayer-algorithms#pending-datagrams)).

### [`Chain.tla`](Chain.tla)
The chain state is represented by a chain store, which is a snapshot of the provable and private stores, to the extent necessary for IBC. Additionally, a chain has dedicated 
datagram containers for: 
1. client, connection, and channel datagrams (given by a set of datagrams),
2. packet datagrams (given by a queue of datagrams that models the order in which the datagrams were submitted by the relayer).

Its transition relation is defined by the formula:
```tla
Next ==
    \/ AdvanceChain 
    \/ HandleIncomingDatagrams
    \/ SendPacket
    \/ AcknowledgePacket
    \/ UNCHANGED vars
```
where:
- `AdvanceChain`: increments the height of the chain,
- `HandleIncomingDatagrams`: dispatches the datagrams to the appropriate handlers. 
This captures the logic of the [routing module](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-026-routing-module).
- `SendPacket`: models user/application-defined calls to send a packet. As this specification does not have a specific application in mind, we abstract away from the packet data, and allow sending packets non-deterministically. 
The packet commitment is written in the chain store, and the sent packet is logged, 
which triggers the relayer to create a `PacketRecv` datagram.
- `AcknowledgePacket`: writes an acknowledgement for a received packet
  in the chain store and on the packet log, which triggers the relayer to create a `PacketAck` datagram.

### [`ICS02ClientHandlers.tla`](ICS02ClientHandlers.tla), [`ICS03ConnectionHandlers.tla`](ICS03ConnectionHandlers.tla), [`ICS04ChannelHandlers.tla`](ICS04ChannelHandlers.tla), [`ICS04PacketHandlers.tla`](ICS04PacketHandlers.tla)
These TLA+ modules contain definitions of 
operators that handle client, connection handshake, channel handshake, and packet 
datagrams, respectively.
These operators capture the logic of the handlers defined in [ICS02](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-002-client-semantics), [ICS03](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-003-connection-semantics), and 
[ICS04](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics).





The module `Relayer.tla` contains the specification of the relayer algorithm. 
The module `Chain.tla` captures the chain logic. 
It extends the modules `ClientHandlers.tla`, 
`ConnectionHandlers.tla`, `ChannelHandlers.tla`, and
`PacketHandlers.tla`, which contain definition of 
operators that handle client, connection handshake, channel handshake, and packet 
datagrams, respectively.
The module `RelayerDefinitions.tla` contains definition of operators that are used across all the 
modules.

## Properties and Invariants

### System-level properties

We specify three kinds of properties for the IBC core protocols in the module [IBCCore.tla](IBCCore.tla):

- `IBCSafety`: Bad datagrams are not used to update the chain stores.

- `IBCDelivery`: If `ChainA` sends a datagram to `ChainB`, then `ChainB` eventually receives the datagram



### Packets

[ICS04](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics) specifies the following list of  ["Desired
Properties"](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics#desired-properties)

#### [Efficiency](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics#efficiency)

Efficiency seems to be too vague to formalize. In particular the
formulation ignores relayers that are the active components in packet
transmission. It is not clear what a suitable way is to formalize it.
  
#### [Exactly-once delivery](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics#exactly-once-delivery)

These properties are also vague as:

* in the absence of a relayer no packets can be delivered
* ignores timeouts
* unspecific what "sent" means. We suggest it means that a packet commitment is written in the provable store (in our model `ChainStore`) rather than executing `SendPacket`.

As a result we suggest that the property should be decomposed into to properties:

* (at most once) For each packer `p`, if a chain performs `RecvPacket(p)` successfully (without abort), it will
		  not perform `RecvPacket(p)` successfully in the future.  
      

* (typical case) If
  * sender and receiver chain are valid, and
  * there is a correct relayer, and 
  * communication is bounded in time, and
  * the `timeoutHeights` and times are luckily chosen, and 
  * the receiver chain does not censor the packet

  then the packet will be delivered.


The second property ignores that timeouts can happen.

If this is the confirmed intended behavior, these properties can be expressed
and verified 
by a slight modification of the specification, in particular, the way in which 
the packet receipts are stored in the chain store (in a set vs. in a sequence).

#### [Ordering](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics#ordering)

- ordered channels: It is not clear what "if packet x is sent before packet y by a channel end on chain A" meant in a context where chain A performs invalid transitions: then a packet with sequence number *i* can be sent after *i+1*. If this happens, the IBC implementation may be broken (depends on the relayer).

In the context of two valid chains, this property can be 
expressed and verified by adding a history 
variable on the receiving side, which is modified by transitions of the receiving chain. 

- no property defined for unordered.

#### [Permissioning](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics#permissioning)

This property is about capabilities. We do not capture capabilities in the TLA+ specification.



### Channel 

As there are no explicit properties regarding channels given in [ICS 04](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics) in textual form, we have formalized that the channel handshake does not deviate from the channel lifecycle provided as a [figure](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-004-channel-and-packet-semantics/channel-state-machine.png). They are given in [IBCCore.tla](IBCCore.tla) under the names

- `ChannelInitSafety`
- `ChannelTryOpenSafety`
- `ChannelOpenSafety`
- `ChannelCloseSafety`

### Connection Handshake

Similar to Channel handshake, we have formalized that the connection handshake does not deviate from the channel lifecycle provided as a [figure](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-003-connection-semantics/state.png). They are given in [IBCCore.tla](IBCCore.tla) under the names

- `ConnectionInitSafety`
- `ConnectionTryOpenSafety`
- `ConnectionOpenSafety`


We formalize [these properties](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-003-connection-semantics#properties--invariants) as follows:
> Connection identifiers are first-come-first-serve: once a connection has been negotiated, a unique identifier pair exists between two chains.

[ICS3-Proto-1-ConnectionUniqueness](https://github.com/informalsystems/hermes/blob/master/docs/spec/connection-handshake/L1_2.md#guarantees) A module accepts (i.e., initializes on) a connection end at most once.

>  The connection handshake cannot be man-in-the-middled by another blockchain's IBC handler.

The scenario is not clear, so we did not formalize it.



## Using the Model

### Constants

The module `IBCCore.tla` is parameterized by the constants:
 - `ClientDatagramsRelayer_i`, for `i in {1, 2}`, a Boolean flag defining if `Relayer_i` creates client datagrams, 
 - `ConnectionDatagramsRelayer_i`, for `i in {1, 2}`, a Boolean flag defining if `Relayer_i` creates connection datagrams,
 - `ChannelDatagramsRelayer_i`, for `i in {1, 2}`, a Boolean flag defining if `Relayer_i` creates channel datagrams,
 - `PacketDatagramsRelayer_i`, for `i in {1, 2}`, a Boolean flag defining if `Relayer_i` creates packet datagrams,
 - `MaxHeight`, a natural number denoting the maximal height of the chains,
 - `MaxVersion`, a natural number denoting the maximal connection / channel version supported,
 - `MaxPacketSeq`, a natural number denoting the maximal packet sequence number,
 - `ChannelOrdering`, a string indicating whether the channels are ordered or unordered 

#### Assigning values to the constants

The Boolean flags, defined as constants in the module `IBCCore.tla`, allow us to run experiments in different settings. For example, if we set both `ClientDatagramsRelayer1` and `ClientDatagramsRelayer2` to `TRUE` in a TLC model, then the two relayers in the system concurrently create datagrams related to client creation and client update, and the model checker will check the temporal properties related to client datagrams.

Observe that the setting where, for example, `ClientDatagramsRelayer1 = TRUE`, `ConnectionDatagramsRelayer2 = TRUE`, `ChannelDatagramsRelayer1 = TRUE`, `PacketDatagramsRelayer1 = TRUE`, and the remaining boolean flags are `FALSE`, is equivalent to having a single relayer.

### Importing the specification into TLA+ toolbox

To import the specification in the TLA+ toolbox and run TLC:
  - add a new spec in TLA+ toolbox with the root-module file `IBCCore.tla` 
  - create a model
  - assign a value to the constants (example values can be found in `IBCCore.cfg`)
  - choose "Temporal formula" as the behavior spec, and use the formula `Spec`
  - add the properties `IBCSafety` and `IBCDelivery`
  - run TLC on the model

#### Basic checks with TLC

We ran TLC using the constants defined in `IBCCore.cfg` and verified the invariant `TypeOK` in 14min and the invariant `IBCInv` in 11min.
As TLC usually takes longer to check safety and liveness properties, we have not 
conducted extensive experiments to check `IBCSafety` and `IBCDelivery` with TLC yet.

#### Apalache

The specification contains type annotations for the 
model checker [Apalache](https://github.com/informalsystems/apalache).
The specification passes the type check using the type checker [Snowcat](https://apalache-mc.org/docs/apalache/typechecker-snowcat.html) 
integrated in Apalache.  

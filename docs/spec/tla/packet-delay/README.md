# TLA+ Specification of IBC Packet Transmission with Packet Delay (deprecated)

This document describes the TLA+ specification of an IBC packet transmission with 
packet delays. 
IBC packet transmission with packet delays ensures that 
packet-related data should be accepted only after some delay has passed since the corresponding header is installed. 
This allows a correct relayer to intervene if the header is from a fork and shutdown the IBC handler, preventing damage at the application level.

This TLA+ specification was used during the [design process](https://github.com/cosmos/cosmos-sdk/pull/7884) of the IBC connection-specified delay, where packet delay was a time duration. 
Later, this design was augmented by adding a second delay parameter, in 
terms of number of blocks; called [hybrid packet delay](https://github.com/cosmos/ibc/issues/539).

## The Model of the Protocol

We model a system where packet datagrams are both **submitted** by a 
relayer and **handled** by a chain after a delay period has passed. 
The system contains the following modules: 
- [IBCPacketDelay.tla](IBCPacketDelay.tla), the main module, which 
instantiates two chains and models the behavior of a correct relayer 
as the environment where the two chains operate;
- [Chain.tla](Chain.tla), which models the behavior of a chain;
- [IBCPacketDelayDefinitions.tla](IBCPacketDelayDefinitions.tla), which contains definitions of operators that are shared between the 
 different modules;
- [ICS04PacketHandlers.tla](ICS04PacketHandlers.tla), which contains definitions of operators that specify packet transmission and packet datagram handling.

### Timestamps

To be able to enforce packet datagram submission and handling after a given delay, 
we introduce a `timestamp` field in the chain store. 
This `timestamp` is initially 1, and is incremented when a chain takes a step, that is, when it advances its height, or when it processes datagrams. 

Further, we need to keep track of the time when a counterparty client height 
is installed on a chain. 
That is, instead of keeping track of a set of counterparty client heights, in the 
chain store, we store for each client height 
the timestamp at which it was installed.
A counterparty client height whose timestamp is 0 has 
not yet been installed on the chain.


### Relayer

In this specification, the relayer is a part of the environment in which the two chains operate. 
We define three actions that the environment (i.e., the relayer) can take:
- `UpdateClients`, which updates the counterparty client 
heights of some chain. This action abstracts the 
transmission of client datagrams.
- `CreateDatagrams`, which creates datagrams depending 
on the packet log. This action scans the packet log and 
adds the created packet datagram to the outgoing packet 
datagram queue of the appropriate chain.
- `SubmitDatagramsWithDelay`, which submits datagrams if 
delay has passed. This action scans the outgoing packet datagram queue 
of a given chain, and 
checks if the `proofHeight` of the datagram is a 
client height that is installed on the chain. 
The following cases are possible:
    - if `proofHeight` is installed, then check if a `MaxDelay` period 
    has passed between the timestamp when the client height was 
    installed and the current `timestamp`, stored in the chain store. If 
    this is the case -- submit the datagram to the incoming packet 
    datagram queue of the chain; otherwise -- do nothing. 
     - if `proofHeight` is not installed, then install the it.

### Packet handlers

On the packet handling side, the chain also checks if the incoming 
`PacketRecv` or `PacketAck` datagram has a valid `proofHeight` field.
This means that the `proofHeight` of the datagram should be installed on the 
chain, and there should be `MaxDelay` period between the timestamp when the `proofHeight` was 
installed and the current `timestamp` of the chain.

### History variable

We define a history variable, called `packetDatagramTimestamp`, where we store 
for each `chainID` and each `proofHeight`, the timestamp of the chain `chainID` when a datagram with this `proofHeight` was processed. 
We use this history variable in the invariant `PacketDatagramsDelay`, 
described below.
 

## Invariants

The module [IBCPacketDelay.tla](IBCPacketDelay.tla) defines the following invariants:
- `TypeOK`, the type invariant,
- `PacketDatagramsDelay`, which ensures that each packet 
datagram is processed after a delay period.

## Using the Model

### Constants 

The module `IBCPacketDelay.tla` is parameterized by the constants:
 - `MaxHeight`, a natural number denoting the maximal height of the chains,
 - `ChannelOrdering`, a string denoting whether the channels are ordered or unordered,
 - `MaxPacketSeq`, a natural number denoting the maximal packet sequence number
 - `MaxDelay`, a natural number denoting the maximal packet delay

### Importing the specification into TLA+ toolbox

To import the specification in the TLA+ toolbox and run TLC:
  - add a new spec in TLA+ toolbox with the root-module file `IBCPacketDelay.tla` 
  - create a model
  - assign a value to the constants (example values can be found in `IBCPacketDelay.cfg`)
  - choose "Temporal formula" as the behavior spec, and use the formula `Spec`
  - choose invariants/properties that should be checked
  - run TLC on the model
  
#### Basic checks with TLC

We ran TLC on `IBCPacketDelay.tla` using the constants defined 
in `IBCPacketDelay.cfg`.
We were able to check the invariants described above within seconds.

#### Apalache

The specification contains type annotations for the 
model checker [Apalache](https://github.com/informalsystems/apalache).
The specification passes the type check using the type checker [Snowcat](https://apalache-mc.org/docs/apalache/typechecker-snowcat.html) 
integrated in Apalache.  



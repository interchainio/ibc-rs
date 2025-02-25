# TLA+ specification of the IBC Core Client Protocol

This document describes the TLA+ models of the core logic of the English specification 
[ICS02](https://github.com/cosmos/ibc/tree/5877197dc03e844542cb8628dd52674a37ca6ff9/spec/ics-002-client-semantics).
We start by discussing [the model of the
protocol](#the-model-of-the-protocol).
Then, we discuss the [invariants](#invariants) that we formalize, and finally, we 
discuss how to [use the model](#using-the-model).

## The Model of the Protocol

We present models two of two different systems, which are used to check 
different invariants:
1. The first system, specified in [ICS02SingleChainEnvironment.tla](ICS02SingleChainEnvironment.tla), consists of a single chain that can
create multiple clients. 
The chain operates in an environment that overapproximates the 
behavior of a correct relayer. 
2. The second system, specified in [ICS02TwoChainsEnvironment.tla](ICS02TwoChainsEnvironment.tla), consists of two chain that can
create multiple clients. 
The relayer is again overapproximated using an environment.

Both systems extend the following modules:
- [Chain.tla](Chain.tla), which models the behavior of a chain running the IBC Core Client Protocol.
- [ICS02ClientHandlers.tla](ICS02ClientHandlers.tla), which contains definitions of 
operators that are used to handle client creation and client update events.
- [ICS02Definitions.tla](ICS02Definitions.tla), which contains definitions of operators that are shared between the 
 different modules, and which are relevant for ICS02.

## Invariants

The module [Chain.tla](Chain.tla) defines the following invariants:
- `TypeOK`, the type invariant,
- `CreatedClientsHaveDifferentIDs`, which ensures that two clients two clients with the same ID cannot be created,
- `UpdatedClientsAreCreated`, which ensures that only created clients can be updated.

These invariants are checked for a system of single chain in [ICS02SingleChainEnvironment.tla](ICS02SingleChainEnvironment.tla), and for a system of two chains in [ICS02TwoChainsEnvironment.tla](ICS02TwoChainsEnvironment.tla). 
Additionally, [ICS02SingleChainEnvironment](ICS02TwoChainsEnvironment.tla) checks the invariant:
- `ClientHeightsAreBelowCounterpartyHeight`, which ensures that the maximum client 
height is less than or equal to the current height of the counterparty chain. 


## Using the Model

### Constants 

The modules `ICS02SingleChainEnvironment.tla` and `ICS02TwoChainsEnvironment.tla`
are parameterized by the constants:
 - `MaxHeight`, a natural number denoting the maximal height of the chains,
 - `NrClientsChainA`, a number of clients that will be created on ChainA
 - `NrClientsChainB`, a number of clients that will be created on ChainB
 - `ClientIDsChainA`, a set of counterparty client IDs for ChainA
 - `ClientIDsChainB`, a set of counterparty client IDs for ChainB

We assume that the sets `ClientIDsChainA` and `ClientIDsChainB` contain distinct 
client IDs.


### Importing the specification into TLA+ toolbox

To import the specification in the TLA+ toolbox and run TLC:
  - add a new spec in TLA+ toolbox with the root-module file `ICS02SingleChainEnvironment.tla` (or `ICS02TwoChainsEnvironment.tla`)
  - create a model
  - assign a value to the constants (example values can be found in `ICS02SingleChainEnvironment.cfg` (or `ICS02TwoChainsEnvironment.tla`))
  - choose "Temporal formula" as the behavior spec, and use the formula `Spec`
  - choose invariants/properties that should be checked
  - run TLC on the model
  
#### Basic checks with TLC

We ran TLC on `ICS02SingleChainEnvironment.tla` using the constants defined 
in `ICS02SingleChainEnvironment.cfg`.
We were able to check the invariants described above within seconds.

#### Apalache

The specification contains type annotations for the 
model checker [Apalache](https://github.com/informalsystems/apalache).
The specification passes the type check using the type checker [Snowcat](https://apalache-mc.org/docs/apalache/typechecker-snowcat.html) 
integrated in Apalache.  


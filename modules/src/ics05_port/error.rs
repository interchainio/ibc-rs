use crate::ics24_host::identifier::PortId;
use flex_error::define_error;

define_error! {
    #[derive(Debug, PartialEq)]
    Error {
        UnknownPort
            { port_id: PortId }
            | e | { format_args!("Port {0} is unknown", e.port_id) },

        ImplementationSpecific
            | _ | { "implementation specific error" },
    }
}

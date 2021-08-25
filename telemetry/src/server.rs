use alloc::sync::Arc;
use std::{error::Error, net::ToSocketAddrs};

use prometheus::{Encoder, TextEncoder};
use rouille::{Request, Response, Server};

use crate::state::TelemetryState;

enum Route {
    Metrics,
    Other,
}

impl Route {
    fn from_request(request: &Request) -> Route {
        if request.url() == "/metrics" {
            Route::Metrics
        } else {
            Route::Other
        }
    }
}

pub fn listen(
    address: impl ToSocketAddrs,
    telemetry_state: Arc<TelemetryState>,
) -> Result<Server<impl Fn(&Request) -> Response>, Box<dyn Error + Send + Sync>> {
    let server = Server::new(address, move |request| {
        match Route::from_request(request) {
            // The prometheus endpoint
            Route::Metrics => {
                let mut buffer = vec![];
                let encoder = TextEncoder::new();
                let metric_families = telemetry_state.gather();
                encoder.encode(&metric_families, &mut buffer).unwrap();

                rouille::Response::from_data(encoder.format_type().to_string(), buffer)
            }

            // Any other route
            // Return an empty response with a 404 status code.
            Route::Other => rouille::Response::empty_404(),
        }
    })?;

    Ok(server)
}

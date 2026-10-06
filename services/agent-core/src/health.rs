//! gRPC health for the Agent Core.
//!
//! Serves the standard gRPC health service. The history store and the backend report their own
//! health into it, so the Agent Core service is serving only while both are healthy and shutdown
//! has not begun. The overall server status stays serving from the moment the server listens until
//! shutdown begins.

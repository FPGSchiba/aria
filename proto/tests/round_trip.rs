use prost::Message;
use proto::agent_core::v1::{
    DecideRequest, DecideResponse, TurnComplete as AgentTurnComplete, decide_response,
};
use proto::gateway::v1::{
    ConverseRequest, ConverseResponse, TurnComplete as GatewayTurnComplete, UserTurn,
    converse_request, converse_response,
};

#[test]
fn test_converse_request_round_trip_with_speaker() {
    let original = ConverseRequest {
        payload: Some(converse_request::Payload::UserTurn(UserTurn {
            speaker_name: Some("Alice".to_string()),
            text: "Hello ARIA, what is the weather today?".to_string(),
        })),
    };

    let encoded = original.encode_to_vec();
    let decoded =
        ConverseRequest::decode(encoded.as_slice()).expect("failed to decode ConverseRequest");

    assert_eq!(decoded, original);
}

#[test]
fn test_converse_request_round_trip_without_speaker() {
    let original = ConverseRequest {
        payload: Some(converse_request::Payload::UserTurn(UserTurn {
            speaker_name: None,
            text: "Turn off the living room lights.".to_string(),
        })),
    };

    let encoded = original.encode_to_vec();
    let decoded = ConverseRequest::decode(encoded.as_slice())
        .expect("failed to decode ConverseRequest without speaker");

    assert_eq!(decoded, original);
}

#[test]
fn test_converse_response_text_delta_round_trip() {
    let original = ConverseResponse {
        payload: Some(converse_response::Payload::TextDelta(
            "I have turned off the lights.".to_string(),
        )),
    };

    let encoded = original.encode_to_vec();
    let decoded = ConverseResponse::decode(encoded.as_slice())
        .expect("failed to decode ConverseResponse text_delta");

    assert_eq!(decoded, original);
}

#[test]
fn test_converse_response_turn_complete_round_trip() {
    let original = ConverseResponse {
        payload: Some(converse_response::Payload::TurnComplete(
            GatewayTurnComplete {},
        )),
    };

    let encoded = original.encode_to_vec();
    let decoded = ConverseResponse::decode(encoded.as_slice())
        .expect("failed to decode ConverseResponse turn_complete");

    assert_eq!(decoded, original);
}

#[test]
fn test_decide_request_round_trip_with_speaker() {
    let original = DecideRequest {
        session_id: "sess_01HZX87654321".to_string(),
        speaker_name: Some("Bob".to_string()),
        text: "What meetings do I have tomorrow?".to_string(),
    };

    let encoded = original.encode_to_vec();
    let decoded =
        DecideRequest::decode(encoded.as_slice()).expect("failed to decode DecideRequest");

    assert_eq!(decoded, original);
}

#[test]
fn test_decide_request_round_trip_without_speaker() {
    let original = DecideRequest {
        session_id: "sess_01HZX87654321".to_string(),
        speaker_name: None,
        text: "What meetings do I have tomorrow?".to_string(),
    };

    let encoded = original.encode_to_vec();
    let decoded = DecideRequest::decode(encoded.as_slice())
        .expect("failed to decode DecideRequest without speaker");

    assert_eq!(decoded, original);
}

#[test]
fn test_decide_response_text_delta_round_trip() {
    let original = DecideResponse {
        payload: Some(decide_response::Payload::TextDelta(
            "You have two meetings scheduled.".to_string(),
        )),
    };

    let encoded = original.encode_to_vec();
    let decoded = DecideResponse::decode(encoded.as_slice())
        .expect("failed to decode DecideResponse text_delta");

    assert_eq!(decoded, original);
}

#[test]
fn test_decide_response_turn_complete_round_trip() {
    let original = DecideResponse {
        payload: Some(decide_response::Payload::TurnComplete(AgentTurnComplete {})),
    };

    let encoded = original.encode_to_vec();
    let decoded = DecideResponse::decode(encoded.as_slice())
        .expect("failed to decode DecideResponse turn_complete");

    assert_eq!(decoded, original);
}

//! Control-channel login/join state machine.
//!
//! This models the stock UE4 server transition around NMT_Join.  NMT_Join has
//! no gameplay payload: it means the client has finished loading the map named
//! by NMT_Welcome.  The engine then SpawnPlayActor()s the player's controller
//! (which runs GameMode Login/PostLogin) and actor replication can begin.

use crate::control::{Hello, Welcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginPhase {
    AwaitHello,
    AwaitLogin,
    Welcomed,
    Joined,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginIdentity {
    /// NMT_Login response string. Prospect's public emulator uses E660A966 as
    /// its challenge, but validation belongs in the backend/auth adapter.
    pub client_response: String,
    /// UE travel/request URL persisted until NMT_Join, where SpawnPlayActor
    /// consumes it.
    pub request_url: String,
    /// Preserve the network identity as an opaque value until the full
    /// FUniqueNetIdRepl codec is wired into the transport layer.
    pub unique_id: String,
    pub platform_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAction {
    SendChallenge(String),
    SendWelcome {
        map: String,
        game_mode: String,
        redirect_url: String,
    },
    SetNetSpeed(i32),
    /// Equivalent boundary to UWorld::SpawnPlayActor succeeding.  The actor
    /// layer should use this event to create PlayerController/PlayerState/Pawn
    /// and open their actor channels.
    BeginActorBootstrap {
        request_url: String,
        unique_id: String,
        platform_name: String,
    },
    IgnoreDuplicateJoin,
    Reject(String),
}

#[derive(Debug, Clone)]
pub struct LoginSession {
    phase: LoginPhase,
    challenge: String,
    map: String,
    game_mode: String,
    redirect_url: String,
    hello: Option<Hello>,
    identity: Option<LoginIdentity>,
    net_speed: Option<i32>,
}

impl LoginSession {
    pub fn new(map: impl Into<String>, game_mode: impl Into<String>) -> Self {
        Self {
            phase: LoginPhase::AwaitHello,
            challenge: "E660A966".to_string(),
            map: map.into(),
            game_mode: game_mode.into(),
            redirect_url: String::new(),
            hello: None,
            identity: None,
            net_speed: None,
        }
    }

    pub fn phase(&self) -> LoginPhase { self.phase }
    pub fn identity(&self) -> Option<&LoginIdentity> { self.identity.as_ref() }
    pub fn net_speed(&self) -> Option<i32> { self.net_speed }

    pub fn on_hello(&mut self, hello: Hello) -> ServerAction {
        if self.phase != LoginPhase::AwaitHello {
            return self.reject("NMT_Hello arrived after login started");
        }
        self.hello = Some(hello);
        self.phase = LoginPhase::AwaitLogin;
        ServerAction::SendChallenge(self.challenge.clone())
    }

    pub fn on_login(&mut self, identity: LoginIdentity) -> ServerAction {
        if self.phase != LoginPhase::AwaitLogin {
            return self.reject("NMT_Login arrived in the wrong phase");
        }
        // Keep the response opaque for now.  The existing API/SignalR adapter
        // will own authentication; the UE transport must not invent DB logic.
        self.identity = Some(identity);
        self.phase = LoginPhase::Welcomed;
        ServerAction::SendWelcome {
            map: self.map.clone(),
            game_mode: self.game_mode.clone(),
            redirect_url: self.redirect_url.clone(),
        }
    }

    pub fn on_netspeed(&mut self, requested: i32) -> ServerAction {
        if self.phase != LoginPhase::Welcomed && self.phase != LoginPhase::Joined {
            return self.reject("NMT_Netspeed arrived before NMT_Welcome");
        }
        // UE clamps this elsewhere; retain the requested value here so policy
        // can be applied by the connection layer.
        self.net_speed = Some(requested);
        ServerAction::SetNetSpeed(requested)
    }

    pub fn on_join(&mut self) -> ServerAction {
        match self.phase {
            LoginPhase::Welcomed => {
                let identity = match self.identity.clone() {
                    Some(v) => v,
                    None => return self.reject("NMT_Join without persisted NMT_Login identity"),
                };
                self.phase = LoginPhase::Joined;
                ServerAction::BeginActorBootstrap {
                    request_url: identity.request_url,
                    unique_id: identity.unique_id,
                    platform_name: identity.platform_name,
                }
            }
            LoginPhase::Joined => ServerAction::IgnoreDuplicateJoin,
            _ => self.reject("NMT_Join arrived before the client was welcomed"),
        }
    }

    pub fn welcome(&self) -> Welcome<'_> {
        Welcome {
            map: &self.map,
            game_mode: &self.game_mode,
            redirect_url: &self.redirect_url,
        }
    }

    fn reject(&mut self, reason: &str) -> ServerAction {
        self.phase = LoginPhase::Failed;
        ServerAction::Reject(reason.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = "/Game/Maps/MP/AlienCaverns/MP_AlienCaverns_P";
    const GAME_MODE: &str = "/Script/Prospect.YGameMode_Match";

    fn hello() -> Hello {
        Hello {
            is_little_endian: 0,
            remote_network_version: 0x1234_5678,
            encryption_token: String::new(),
        }
    }

    fn identity() -> LoginIdentity {
        LoginIdentity {
            client_response: "opaque-client-response".into(),
            request_url: format!("{MAP}?Name=LocalTest"),
            unique_id: "test-user-1".into(),
            platform_name: "WIN".into(),
        }
    }

    #[test]
    fn full_control_login_reaches_actor_bootstrap_only_after_join() {
        let mut s = LoginSession::new(MAP, GAME_MODE);
        assert_eq!(s.phase(), LoginPhase::AwaitHello);

        assert_eq!(
            s.on_hello(hello()),
            ServerAction::SendChallenge("E660A966".into())
        );
        assert_eq!(s.phase(), LoginPhase::AwaitLogin);

        let expected_identity = identity();
        assert_eq!(
            s.on_login(expected_identity.clone()),
            ServerAction::SendWelcome {
                map: MAP.into(),
                game_mode: GAME_MODE.into(),
                redirect_url: String::new(),
            }
        );
        assert_eq!(s.phase(), LoginPhase::Welcomed);
        assert_eq!(s.identity(), Some(&expected_identity));

        assert_eq!(s.on_netspeed(1_200_000), ServerAction::SetNetSpeed(1_200_000));
        assert_eq!(s.net_speed(), Some(1_200_000));

        assert_eq!(
            s.on_join(),
            ServerAction::BeginActorBootstrap {
                request_url: format!("{MAP}?Name=LocalTest"),
                unique_id: "test-user-1".into(),
                platform_name: "WIN".into(),
            }
        );
        assert_eq!(s.phase(), LoginPhase::Joined);
    }

    #[test]
    fn join_is_rejected_before_welcome() {
        let mut s = LoginSession::new(MAP, GAME_MODE);
        assert!(matches!(s.on_join(), ServerAction::Reject(_)));
        assert_eq!(s.phase(), LoginPhase::Failed);
    }

    #[test]
    fn duplicate_join_does_not_spawn_a_second_player() {
        let mut s = LoginSession::new(MAP, GAME_MODE);
        let _ = s.on_hello(hello());
        let _ = s.on_login(identity());
        let _ = s.on_join();
        assert_eq!(s.on_join(), ServerAction::IgnoreDuplicateJoin);
        assert_eq!(s.phase(), LoginPhase::Joined);
    }
}

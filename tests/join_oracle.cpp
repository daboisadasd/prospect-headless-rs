#include <cassert>
#include <cstdint>
#include <iostream>
#include <optional>
#include <string>

namespace ue {
enum class Phase { AwaitHello, AwaitLogin, Welcomed, Joined, Failed };
enum class Action { Challenge, Welcome, SetNetSpeed, BeginActorBootstrap, IgnoreDuplicateJoin, Reject };

struct Identity {
    std::string response;
    std::string request_url;
    std::string unique_id;
    std::string platform;
};

struct Session {
    Phase phase = Phase::AwaitHello;
    std::optional<Identity> identity;
    std::optional<std::int32_t> net_speed;

    Action hello() {
        if (phase != Phase::AwaitHello) return fail();
        phase = Phase::AwaitLogin;
        return Action::Challenge;
    }
    Action login(Identity id) {
        if (phase != Phase::AwaitLogin) return fail();
        identity = std::move(id);
        phase = Phase::Welcomed;
        return Action::Welcome;
    }
    Action netspeed(std::int32_t speed) {
        if (phase != Phase::Welcomed && phase != Phase::Joined) return fail();
        net_speed = speed;
        return Action::SetNetSpeed;
    }
    Action join() {
        if (phase == Phase::Joined) return Action::IgnoreDuplicateJoin;
        if (phase != Phase::Welcomed || !identity) return fail();
        // This is the UE boundary corresponding to:
        //   FURL InURL(nullptr, *Connection->RequestURL, TRAVEL_Absolute);
        //   Connection->PlayerController = SpawnPlayActor(
        //       Connection, ROLE_AutonomousProxy, InURL, Connection->PlayerId, ...);
        phase = Phase::Joined;
        return Action::BeginActorBootstrap;
    }
private:
    Action fail() { phase = Phase::Failed; return Action::Reject; }
};
} // namespace ue

int main() {
    constexpr auto map = "/Game/Maps/MP/AlienCaverns/MP_AlienCaverns_P";

    // Bad ordering must fail deterministically.
    {
        ue::Session s;
        assert(s.join() == ue::Action::Reject);
        assert(s.phase == ue::Phase::Failed);
    }

    // Exact semantic path used by an ordinary UE client after map load.
    ue::Session s;
    assert(s.hello() == ue::Action::Challenge);
    assert(s.phase == ue::Phase::AwaitLogin);

    ue::Identity id{
        .response = "opaque-response",
        .request_url = std::string(map) + "?Name=LocalOracle",
        .unique_id = "test-user-1",
        .platform = "WIN",
    };
    assert(s.login(id) == ue::Action::Welcome);
    assert(s.phase == ue::Phase::Welcomed);
    assert(s.identity && s.identity->request_url == std::string(map) + "?Name=LocalOracle");

    assert(s.netspeed(1'200'000) == ue::Action::SetNetSpeed);
    assert(s.net_speed && *s.net_speed == 1'200'000);

    // NMT_Join has no payload. Success means actor bootstrap starts; there is
    // no synthetic server-side NMT_Join response in this model.
    assert(s.join() == ue::Action::BeginActorBootstrap);
    assert(s.phase == ue::Phase::Joined);

    // A retransmitted Join must never create another PlayerController.
    assert(s.join() == ue::Action::IgnoreDuplicateJoin);

    std::cout << "PASS: Hello -> Login -> Welcome -> Netspeed -> Join -> ActorBootstrap\n";
    std::cout << "PASS: early Join rejected\n";
    std::cout << "PASS: duplicate Join does not double-spawn\n";
    return 0;
}

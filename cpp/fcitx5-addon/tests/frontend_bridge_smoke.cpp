#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <cassert>
#include <string>
#include <string_view>

using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::DaemonClient;
using vinput_fcitx_bridge::FrontendBridge;

class FakeDaemonClient final : public DaemonClient {
public:
  bool StartRecording(std::string *error) override {
    ++start_recording_calls;
    if (!start_ok && error) {
      *error = start_error;
    }
    return start_ok;
  }

  bool StartCommandRecording(std::string_view selected_text,
                             std::string *error) override {
    ++start_command_calls;
    last_selected_text = std::string(selected_text);
    if (!start_ok && error) {
      *error = start_error;
    }
    return start_ok;
  }

  bool StopRecording(std::string_view scene_id, std::string *payload_json,
                     std::string *error) override {
    ++stop_calls;
    last_scene_id = std::string(scene_id);
    if (!stop_ok) {
      if (error) {
        *error = "stop failed";
      }
      return false;
    }
    if (payload_json) {
      *payload_json = next_payload_json;
    }
    return true;
  }

  bool start_ok = true;
  bool stop_ok = true;
  std::string start_error;
  std::string next_payload_json =
      R"({"commit_text":"mock recognition result","candidates":[{"text":"mock recognition result","source":"raw"}]})";
  int start_recording_calls = 0;
  int start_command_calls = 0;
  int stop_calls = 0;
  std::string last_selected_text;
  std::string last_scene_id;
};

int main() {
  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    const auto start = bridge.StartNormal(&client);
    assert(start.kind == BridgeOutcome::Kind::Preedit);
    assert(start.text == "... Recording ...");
    assert(bridge.recording());
    assert(!bridge.command_mode());
    assert(client.start_recording_calls == 1);

    const auto stop = bridge.Stop(&client, "default-scene");
    assert(stop.kind == BridgeOutcome::Kind::Commit);
    assert(stop.text == "mock recognition result");
    assert(stop.payload.commit_text == "mock recognition result");
    assert(!stop.command_mode);
    assert(!bridge.recording());
    assert(client.last_scene_id == "default-scene");
  }

  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    const auto start = bridge.StartCommand(&client, "selected text");
    assert(start.kind == BridgeOutcome::Kind::Preedit);
    assert(start.text == "... Commanding ...");
    assert(bridge.recording());
    assert(bridge.command_mode());
    assert(client.start_command_calls == 1);
    assert(client.last_selected_text == "selected text");

    const auto stop = bridge.Stop(&client, "command-scene");
    assert(stop.kind == BridgeOutcome::Kind::Commit);
    assert(stop.command_mode);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.next_payload_json =
        R"({"commit_text":"polished 1","candidates":[{"text":"raw transcript","source":"raw"},{"text":"polished 1","source":"llm"},{"text":"polished 2","source":"llm"}]})";
    FrontendBridge bridge;

    assert(bridge.StartNormal(&client).kind == BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(&client, "menu-scene");
    assert(stop.kind == BridgeOutcome::Kind::CandidateMenu);
    assert(stop.payload.candidates.size() == 3);
    assert(!stop.command_mode);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.next_payload_json =
        R"({"commit_text":"polished 1","candidates":[{"text":"raw transcript","source":"raw"},{"text":"polished 1","source":"llm"},{"text":"polished 2","source":"llm"}]})";
    FrontendBridge bridge;

    assert(bridge.StartCommand(&client, "selected text").kind ==
           BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(&client, "command-menu-scene");
    assert(stop.kind == BridgeOutcome::Kind::CandidateMenu);
    assert(stop.command_mode);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.next_payload_json = R"({"commit_text":"","candidates":[]})";
    FrontendBridge bridge;

    assert(bridge.StartNormal(&client).kind == BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(&client, "empty-scene");
    assert(stop.kind == BridgeOutcome::Kind::Clear);
    assert(!stop.command_mode);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.next_payload_json = R"({"candidates":[{"text":"","source":"cancel"}]})";
    FrontendBridge bridge;

    assert(bridge.StartCommand(&client, "selected text").kind ==
           BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(&client, "cancel-scene");
    assert(stop.kind == BridgeOutcome::Kind::Clear);
    assert(stop.command_mode);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    const auto stop = bridge.Stop(&client, "not-recording-scene");
    assert(stop.kind == BridgeOutcome::Kind::None);
    assert(client.stop_calls == 0);
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.stop_ok = false;
    FrontendBridge bridge;

    assert(bridge.StartNormal(&client).kind == BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(&client, "failing-scene");
    assert(stop.kind == BridgeOutcome::Kind::Error);
    assert(stop.text == "stop failed");
    assert(client.stop_calls == 1);
    assert(client.last_scene_id == "failing-scene");
    assert(!bridge.recording());
    assert(!bridge.command_mode());
  }

  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    assert(bridge.StartCommand(&client, "selected text").kind ==
           BridgeOutcome::Kind::Preedit);
    assert(bridge.recording());
    assert(bridge.command_mode());
    bridge.Reset();
    assert(!bridge.recording());
    assert(!bridge.command_mode());
    const auto stop = bridge.Stop(&client, "after-reset-scene");
    assert(stop.kind == BridgeOutcome::Kind::None);
    assert(client.stop_calls == 0);
  }

  {
    FrontendBridge bridge;

    const auto start = bridge.StartNormal(nullptr);
    assert(start.kind == BridgeOutcome::Kind::Error);
    assert(start.text == "Voice input daemon is unavailable.");
    assert(!bridge.recording());
  }

  {
    FrontendBridge bridge;

    const auto start = bridge.StartCommand(nullptr, "selected text");
    assert(start.kind == BridgeOutcome::Kind::Error);
    assert(start.text == "Voice input daemon is unavailable.");
    assert(!bridge.recording());
    assert(!bridge.command_mode());
  }

  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    assert(bridge.StartCommand(&client, "selected text").kind ==
           BridgeOutcome::Kind::Preedit);
    const auto stop = bridge.Stop(nullptr, "missing-client-scene");
    assert(stop.kind == BridgeOutcome::Kind::Error);
    assert(stop.text == "Voice input daemon is unavailable.");
    assert(client.stop_calls == 0);
    assert(!bridge.recording());
    assert(!bridge.command_mode());
  }

  {
    FakeDaemonClient client;
    client.start_ok = false;
    client.start_error = "start failed";
    FrontendBridge bridge;

    const auto start = bridge.StartNormal(&client);
    assert(start.kind == BridgeOutcome::Kind::Error);
    assert(start.text == "start failed");
    assert(!bridge.recording());
  }

  {
    FakeDaemonClient client;
    client.start_ok = false;
    client.start_error = "command start failed";
    FrontendBridge bridge;

    const auto start = bridge.StartCommand(&client, "selected text");
    assert(start.kind == BridgeOutcome::Kind::Error);
    assert(start.text == "command start failed");
    assert(client.start_command_calls == 1);
    assert(client.last_selected_text == "selected text");
    assert(!bridge.recording());
    assert(!bridge.command_mode());
  }

  {
    FakeDaemonClient client;
    FrontendBridge bridge;

    const auto start = bridge.StartCommand(&client, "");
    assert(start.kind == BridgeOutcome::Kind::Error);
    assert(start.text == "Please select text first.");
    assert(client.start_command_calls == 0);
  }

  return 0;
}

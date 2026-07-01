#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/scene_defaults.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <thread>

using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FrontendBridge;
using vinput_fcitx_bridge::kDefaultCommandSceneId;
using vinput_fcitx_bridge::kDefaultNormalSceneId;
using vinput_fcitx_bridge::SdBusDaemonClient;

namespace {

std::unique_ptr<SdBusDaemonClient> ConnectWithRetry(std::string *error) {
  for (int attempt = 0; attempt < 50; ++attempt) {
    auto client = SdBusDaemonClient::ConnectSession(error);
    if (client != nullptr) {
      return client;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
  }
  return nullptr;
}

std::string ExpectedText(const char *env_name, const char *fallback) {
  const char *value = std::getenv(env_name);
  return value == nullptr ? std::string(fallback) : std::string(value);
}

std::chrono::milliseconds RecordDelay() {
  const char *value = std::getenv("VINPUT_DBUS_SMOKE_RECORD_MS");
  if (value == nullptr) {
    return std::chrono::milliseconds(0);
  }
  char *end = nullptr;
  const long delay_ms = std::strtol(value, &end, 10);
  if (end == value || delay_ms <= 0) {
    return std::chrono::milliseconds(0);
  }
  return std::chrono::milliseconds(delay_ms);
}

void WaitForRecording(std::chrono::milliseconds delay) {
  if (delay.count() > 0) {
    std::this_thread::sleep_for(delay);
  }
}

} // namespace

int main() {
  std::string error;
  auto client = ConnectWithRetry(&error);
  if (client == nullptr) {
    std::cerr << "connect failed: " << error << '\n';
    return 1;
  }

  const auto record_delay = RecordDelay();

  FrontendBridge normal_bridge;
  auto normal_start = normal_bridge.StartNormal(client.get());
  if (normal_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "normal start failed: " << normal_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);

  const auto expected_normal_text =
      ExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_NORMAL", "mock recognition result");
  auto normal_stop = normal_bridge.Stop(client.get(), kDefaultNormalSceneId);
  if (normal_stop.kind != BridgeOutcome::Kind::Commit ||
      normal_stop.text != expected_normal_text) {
    std::cerr << "normal stop did not produce expected commit text: "
              << normal_stop.text << '\n';
    return 1;
  }

  if (normal_bridge.recording() || normal_bridge.command_mode() ||
      normal_stop.command_mode) {
    std::cerr << "normal stop did not reset bridge state\n";
    return 1;
  }

  FrontendBridge command_bridge;
  auto command_start = command_bridge.StartCommand(client.get(), "selected text");
  if (command_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "command start failed: " << command_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);

  const auto expected_command_text = ExpectedText(
      "VINPUT_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");
  auto command_stop = command_bridge.Stop(client.get(), kDefaultCommandSceneId);
  if (command_stop.kind != BridgeOutcome::Kind::Commit ||
      command_stop.text != expected_command_text) {
    std::cerr << "command stop did not produce expected commit text: "
              << command_stop.text << '\n';
    return 1;
  }

  if (command_bridge.recording() || command_bridge.command_mode() ||
      !command_stop.command_mode) {
    std::cerr << "command stop did not reset bridge state\n";
    return 1;
  }

  std::cout << normal_stop.text << '\n' << command_stop.text << '\n';
  return 0;
}

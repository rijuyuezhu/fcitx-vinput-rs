#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <chrono>
#include <iostream>
#include <memory>
#include <string>
#include <thread>

using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FrontendBridge;
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

} // namespace

int main() {
  std::string error;
  auto client = ConnectWithRetry(&error);
  if (client == nullptr) {
    std::cerr << "connect failed: " << error << '\n';
    return 1;
  }

  FrontendBridge bridge;
  auto start = bridge.StartNormal(client.get());
  if (start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "start failed: " << start.text << '\n';
    return 1;
  }

  auto stop = bridge.Stop(client.get(), "raw");
  if (stop.kind != BridgeOutcome::Kind::Commit || stop.text.empty()) {
    std::cerr << "stop did not produce commit text: " << stop.text << '\n';
    return 1;
  }

  std::cout << stop.text << '\n';
  return 0;
}

#pragma once

#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <memory>
#include <string>
#include <string_view>

struct sd_bus;

namespace vinput_fcitx_bridge {

class SdBusDaemonClient final : public DaemonClient {
public:
  static std::unique_ptr<SdBusDaemonClient> ConnectSession(std::string *error);

  ~SdBusDaemonClient() override;
  SdBusDaemonClient(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient &operator=(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient(SdBusDaemonClient &&) = delete;
  SdBusDaemonClient &operator=(SdBusDaemonClient &&) = delete;

  bool StartRecording(std::string *error) override;
  bool StartCommandRecording(std::string_view selected_text,
                             std::string *error) override;
  bool StopRecording(std::string_view scene_id, std::string *payload_json,
                     std::string *error) override;

private:
  explicit SdBusDaemonClient(sd_bus *bus);

  bool CallNoReply(std::string_view method, std::string *error);
  bool CallNoReplyWithString(std::string_view method, std::string_view value,
                             std::string *error);
  bool CallStringReplyWithString(std::string_view method, std::string_view value,
                                 std::string *reply, std::string *error);

  sd_bus *bus_ = nullptr;
};

} // namespace vinput_fcitx_bridge

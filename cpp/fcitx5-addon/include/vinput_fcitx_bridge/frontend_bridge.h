#pragma once

#include "vinput_fcitx_bridge/recognition_payload.h"

#include <cstdint>
#include <string>
#include <string_view>

namespace vinput_fcitx_bridge {

class DaemonClient {
public:
  virtual ~DaemonClient() = default;

  virtual bool StartRecording(std::string *error) = 0;
  virtual bool StartCommandRecording(std::string_view selected_text,
                                     std::string *error) = 0;
  virtual bool StopRecording(std::string_view scene_id, std::string *payload_json,
                             std::string *error) = 0;
};

struct BridgeOutcome {
  enum class Kind : std::uint8_t { None, Preedit, Commit, CandidateMenu, Error };

  Kind kind = Kind::None;
  std::string text;
  RecognitionPayload payload;
  bool command_mode = false;
};

class FrontendBridge {
public:
  BridgeOutcome StartNormal(DaemonClient *client);
  BridgeOutcome StartCommand(DaemonClient *client, std::string_view selected_text);
  BridgeOutcome Stop(DaemonClient *client, std::string_view scene_id);
  void Reset();

  bool recording() const {
    return recording_;
  }
  bool command_mode() const {
    return command_mode_;
  }

private:
  bool recording_ = false;
  bool command_mode_ = false;
  std::string selected_text_;
};

} // namespace vinput_fcitx_bridge

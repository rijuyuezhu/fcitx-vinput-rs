#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr std::string_view kRecordingPreedit = "... Recording ...";
constexpr std::string_view kCommandingPreedit = "... Commanding ...";
constexpr std::string_view kNoSelectionError = "Please select text first.";
constexpr std::string_view kDaemonUnavailableError =
    "Voice input daemon is unavailable.";

BridgeOutcome Preedit(std::string_view text) {
  return BridgeOutcome{BridgeOutcome::Kind::Preedit, std::string(text), {}};
}

BridgeOutcome Error(std::string_view text) {
  return BridgeOutcome{BridgeOutcome::Kind::Error, std::string(text), {}};
}

BridgeOutcome Clear(bool command_mode) {
  return BridgeOutcome{BridgeOutcome::Kind::Clear, {}, {}, command_mode};
}

BridgeOutcome Commit(std::string text, RecognitionPayload payload, bool command_mode) {
  return BridgeOutcome{BridgeOutcome::Kind::Commit, std::move(text), std::move(payload),
                       command_mode};
}

BridgeOutcome CandidateMenu(RecognitionPayload payload, bool command_mode) {
  return BridgeOutcome{
      BridgeOutcome::Kind::CandidateMenu, {}, std::move(payload), command_mode};
}

std::string FallbackError(const std::string &error) {
  return error.empty() ? std::string(kDaemonUnavailableError) : error;
}

} // namespace

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client) {
  return StartNormalWithScene(client, std::nullopt);
}

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client,
                                          std::string_view scene_id) {
  return StartNormalWithScene(client, std::optional<std::string_view>(scene_id));
}

BridgeOutcome
FrontendBridge::StartNormalWithScene(DaemonClient *client,
                                     std::optional<std::string_view> scene_id) {
  if (!client) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  std::string error;
  if (!client->StartRecording(&error)) {
    Reset();
    return Error(FallbackError(error));
  }

  recording_ = true;
  command_mode_ = false;
  selected_text_.clear();
  if (scene_id.has_value()) {
    active_scene_id_ = std::string(*scene_id);
  } else {
    active_scene_id_.reset();
  }
  return Preedit(kRecordingPreedit);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text) {
  return StartCommandWithScene(client, selected_text, std::nullopt);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return StartCommandWithScene(client, selected_text,
                               std::optional<std::string_view>(scene_id));
}

BridgeOutcome
FrontendBridge::StartCommandWithScene(DaemonClient *client,
                                      std::string_view selected_text,
                                      std::optional<std::string_view> scene_id) {
  if (selected_text.empty()) {
    Reset();
    return Error(kNoSelectionError);
  }
  if (!client) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  std::string error;
  if (!client->StartCommandRecording(selected_text, &error)) {
    Reset();
    return Error(FallbackError(error));
  }

  recording_ = true;
  command_mode_ = true;
  selected_text_ = std::string(selected_text);
  if (scene_id.has_value()) {
    active_scene_id_ = std::string(*scene_id);
  } else {
    active_scene_id_.reset();
  }
  return Preedit(kCommandingPreedit);
}

BridgeOutcome FrontendBridge::Stop(DaemonClient *client, std::string_view scene_id) {
  if (!recording_) {
    return BridgeOutcome{};
  }
  if (!client) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  const bool was_command_mode = command_mode_;

  const std::string stop_scene_id = active_scene_id_.value_or(std::string(scene_id));

  std::string payload_json;
  std::string error;
  if (!client->StopRecording(stop_scene_id, &payload_json, &error)) {
    Reset();
    return Error(FallbackError(error));
  }

  auto plan = MakeCommitPlan(payload_json);
  Reset();
  if (plan.payload.commit_text.empty()) {
    return Clear(was_command_mode);
  }
  if (plan.show_candidate_menu) {
    return CandidateMenu(std::move(plan.payload), was_command_mode);
  }
  auto commit_text = plan.payload.commit_text;
  return Commit(std::move(commit_text), std::move(plan.payload), was_command_mode);
}

void FrontendBridge::Reset() {
  recording_ = false;
  command_mode_ = false;
  selected_text_.clear();
  active_scene_id_.reset();
}

} // namespace vinput_fcitx_bridge

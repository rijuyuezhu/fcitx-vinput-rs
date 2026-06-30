#include "vinput_fcitx_bridge/fcitx_addon.h"

#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/surroundingtext.h>

#include <utility>

namespace vinput_fcitx_bridge {
namespace {

std::string SelectedTextFromInputContext(fcitx::InputContext *ic) {
  if (ic == nullptr || !ic->surroundingText().isValid()) {
    return {};
  }
  return ic->surroundingText().selectedText();
}

} // namespace

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance) : instance_(instance) {
  if (instance_ != nullptr) {
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PostInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
  }
}

SdBusDaemonClient *FcitxVinputAddon::EnsureDaemonClient(std::string *error) {
  if (daemon_client_ == nullptr) {
    daemon_client_ = SdBusDaemonClient::ConnectSession(error);
  }
  return daemon_client_.get();
}

AppliedOutcome FcitxVinputAddon::ApplyDaemonUnavailable(fcitx::InputContext *ic,
                                                        std::string error) {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Error;
  outcome.text =
      error.empty() ? "Voice input daemon is unavailable." : std::move(error);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyBridgeOutcome(fcitx::InputContext *ic,
                                                    const BridgeOutcome &outcome) {
  if (outcome.kind == BridgeOutcome::Kind::Error) {
    daemon_client_.reset();
  }
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

AppliedOutcome FcitxVinputAddon::TriggerNormal(fcitx::InputContext *ic,
                                               std::string_view scene_id) {
  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording() ? bridge_.Stop(client, scene_id)
                                     : bridge_.StartNormal(client);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::TriggerCommand(fcitx::InputContext *ic,
                                                std::string_view selected_text,
                                                std::string_view scene_id) {
  if (!bridge_.recording() && selected_text.empty()) {
    return ApplyBridgeOutcome(ic, bridge_.StartCommand(nullptr, selected_text));
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording() ? bridge_.Stop(client, scene_id)
                                     : bridge_.StartCommand(client, selected_text);
  return ApplyBridgeOutcome(ic, outcome);
}

void FcitxVinputAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  if (trigger_policy_.IsNormalTrigger(key_event)) {
    TriggerNormal(key_event.inputContext());
    key_event.filterAndAccept();
    return;
  }

  if (trigger_policy_.IsCommandTrigger(key_event)) {
    TriggerCommand(key_event.inputContext(),
                   SelectedTextFromInputContext(key_event.inputContext()));
    key_event.filterAndAccept();
  }
}

} // namespace vinput_fcitx_bridge

#include "vinput_fcitx_bridge/fcitx_addon.h"

#include <fcitx/event.h>

#include <utility>

namespace vinput_fcitx_bridge {

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance) : instance_(instance) {
  if (instance_ != nullptr) {
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PostInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
  }
}

AppliedOutcome FcitxVinputAddon::TriggerNormal(fcitx::InputContext *ic,
                                               std::string_view scene_id) {
  if (daemon_client_ == nullptr) {
    std::string error;
    daemon_client_ = SdBusDaemonClient::ConnectSession(&error);
    if (daemon_client_ == nullptr) {
      BridgeOutcome outcome;
      outcome.kind = BridgeOutcome::Kind::Error;
      outcome.text =
          error.empty() ? "Voice input daemon is unavailable." : std::move(error);
      return ApplyBridgeOutcomeToInputContext(outcome, ic);
    }
  }

  auto outcome = bridge_.recording() ? bridge_.Stop(daemon_client_.get(), scene_id)
                                     : bridge_.StartNormal(daemon_client_.get());
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

void FcitxVinputAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  if (!trigger_policy_.IsNormalTrigger(key_event)) {
    return;
  }

  TriggerNormal(key_event.inputContext());
  key_event.filterAndAccept();
}

} // namespace vinput_fcitx_bridge

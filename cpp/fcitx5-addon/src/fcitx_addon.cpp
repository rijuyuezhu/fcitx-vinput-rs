#include "vinput_fcitx_bridge/fcitx_addon.h"

#include <utility>

namespace vinput_fcitx_bridge {

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance) : instance_(instance) {}

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

} // namespace vinput_fcitx_bridge

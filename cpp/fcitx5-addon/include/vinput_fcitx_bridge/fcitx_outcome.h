#pragma once

#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <cstdint>

namespace fcitx {
class InputContext;
}

namespace vinput_fcitx_bridge {

enum class AppliedOutcome : std::uint8_t {
  None,
  Preedit,
  Clear,
  Commit,
  CandidateMenu
};

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *ic);

} // namespace vinput_fcitx_bridge

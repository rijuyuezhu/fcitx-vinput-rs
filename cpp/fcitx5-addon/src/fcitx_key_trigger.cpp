#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

#include <utility>

namespace vinput_fcitx_bridge {

FcitxKeyTriggerPolicy::FcitxKeyTriggerPolicy(fcitx::Key normal_trigger)
    : normal_trigger_(std::move(normal_trigger)) {}

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

} // namespace vinput_fcitx_bridge

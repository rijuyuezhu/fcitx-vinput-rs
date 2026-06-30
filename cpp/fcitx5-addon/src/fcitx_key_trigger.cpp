#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

namespace vinput_fcitx_bridge {

FcitxKeyTriggerPolicy::FcitxKeyTriggerPolicy(fcitx::Key normal_trigger,
                                             fcitx::Key command_trigger)
    : normal_trigger_(normal_trigger), command_trigger_(command_trigger) {}

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

bool FcitxKeyTriggerPolicy::IsCommandTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(command_trigger_);
}

} // namespace vinput_fcitx_bridge

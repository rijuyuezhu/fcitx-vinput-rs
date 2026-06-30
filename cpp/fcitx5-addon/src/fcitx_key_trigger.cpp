#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

namespace vinput_fcitx_bridge {

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

} // namespace vinput_fcitx_bridge

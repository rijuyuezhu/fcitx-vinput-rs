#pragma once

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

namespace vinput_fcitx_bridge {

class FcitxKeyTriggerPolicy {
public:
  explicit FcitxKeyTriggerPolicy(
      fcitx::Key normal_trigger = fcitx::Key(FcitxKey_Control_R));

  bool IsNormalTrigger(const fcitx::KeyEvent &event) const;

private:
  fcitx::Key normal_trigger_;
};

} // namespace vinput_fcitx_bridge

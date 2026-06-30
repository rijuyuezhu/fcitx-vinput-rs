#pragma once

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

namespace vinput_fcitx_bridge {

class FcitxKeyTriggerPolicy {
public:
  explicit FcitxKeyTriggerPolicy(
      fcitx::Key normal_trigger = fcitx::Key(FcitxKey_Control_R),
      fcitx::Key command_trigger = fcitx::Key(FcitxKey_F10));

  bool IsNormalTrigger(const fcitx::KeyEvent &event) const;
  bool IsCommandTrigger(const fcitx::KeyEvent &event) const;

private:
  fcitx::Key normal_trigger_;
  fcitx::Key command_trigger_;
};

} // namespace vinput_fcitx_bridge

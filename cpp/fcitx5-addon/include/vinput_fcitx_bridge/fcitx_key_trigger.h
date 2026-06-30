#pragma once

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

namespace vinput_fcitx_bridge {

class FcitxKeyTriggerPolicy {
public:
  bool IsNormalTrigger(const fcitx::KeyEvent &event) const;

private:
  fcitx::Key normal_trigger_{FcitxKey_Control_R};
};

} // namespace vinput_fcitx_bridge

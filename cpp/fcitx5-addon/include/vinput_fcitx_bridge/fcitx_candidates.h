#pragma once

#include "vinput_fcitx_bridge/recognition_payload.h"

#include <fcitx/candidatelist.h>

#include <memory>
#include <string>

namespace vinput_fcitx_bridge {

std::string ResultCandidateComment(const Candidate &candidate, std::size_t llm_index);

std::unique_ptr<fcitx::CommonCandidateList>
BuildResultCandidateList(const RecognitionPayload &payload);

} // namespace vinput_fcitx_bridge

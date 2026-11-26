# Evaluated by CPack separately for each selected generator.
if(CPACK_GENERATOR MATCHES "^(DEB|RPM)$" AND NOT CPACK_PACKAGE_CONTACT)
  message(FATAL_ERROR "Set CLIVUU_PACKAGE_CONTACT (or PACKAGER before configuring) to a maintainer name and email")
endif()
if(CPACK_GENERATOR STREQUAL "DEB")
  find_program(SHLIBDEPS_EXECUTABLE dpkg-shlibdeps REQUIRED)
endif()

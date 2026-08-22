# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file Copyright.txt or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION 3.5)

file(MAKE_DIRECTORY
  "/Users/jdylanstewart/.espressif/esp-idf/v5.2.2/components/bootloader/subproject"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/tmp"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/src/bootloader-stamp"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/src"
  "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/src/bootloader-stamp"
)

set(configSubDirs )
foreach(subDir IN LISTS configSubDirs)
    file(MAKE_DIRECTORY "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/src/bootloader-stamp/${subDir}")
endforeach()
if(cfgdir)
  file(MAKE_DIRECTORY "/Users/jdylanstewart/gitRepos/doorbell-sms/firmware-c6/target/riscv32imac-esp-espidf/debug/build/esp-idf-sys-e1037053b7c2ad08/out/build/bootloader-prefix/src/bootloader-stamp${cfgdir}") # cfgdir has leading slash
endif()

@echo off
setlocal enabledelayedexpansion

:: if the mod folder exists before repacking, keep it for convenience (mostly for modders)
:: if folder was made during repacking phase, delete it after to avoid confusion/save space
set /a keepDir = 0

:: dont allow anything other than .pak files to be processed
set fileExt=none

:: check if anything was dropped into this script
if "%~1"=="" (
    echo No file or directory was provided. Don't forget: drag and drop the .pak file/folder into this script!
    pause
    exit /b
)

:: if more than one thing was provided, deal with them properly
for %%F in (%*) do (
    if exist "%%F" (
        if exist "%%F\*" (
            echo ======================================
            echo Processing folder: %%F
            @pushd %~dp0
            :: Generate the .pak file from folder
            .\repak.exe --aes-key 0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74 pack %%F --version V11 --verbose --patch-uasset --compression Oodle

            :: Get folder name
            set "folderName=%%~nF"
            set "has9999999=0"
            set "hasP=0"

            :: Check if _9999999 is already present
            echo !folderName! | findstr /i "_9999999" >nul
            if !errorlevel! == 0 set "has9999999=1"

            :: Check if _P is already present
            echo !folderName! | findstr /i "_P" >nul
            if !errorlevel! == 0 set "hasP=1"

            :: Construct new name
            set "newName=!folderName!"
            if !has9999999! == 0 set "newName=!newName!_9999999"
            if !hasP! == 0 set "newName=!newName!_P"

            :: Ensure order (_9999999 before _P)
            set "newName=!newName:_P_9999999=_9999999_P!"

            set "newName=!newName!.pak"
            echo Renaming .pak to !newName!
            ren "%%~dpF\%%~nF.pak" "!newName!"

            @popd
        ) else (
            set "fileExt=%%~xF"
            :: convert to lowercase
            for %%A in (!fileExt!) do set "fileExt=%%A"

            if /i "!fileExt!"==".pak" (
                echo Processing .pak file: %%F

                if exist "%%~dpnF\*" ( set /a keepDir = 1 )

                @pushd %%~dpF
                :: Unpack the .pak file
                .\repak.exe --aes-key 0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74 unpack %%F --verbose
                @del %%F

                :: Pack the folder back with the new name
                .\repak.exe --aes-key 0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74 pack "%%~dpnF" --version V11 --verbose --patch-uasset --compression Oodle

                :: Get folder name
                set "folderName=%%~nF"
                set "has9999999=0"
                set "hasP=0"

                :: Check if _9999999 is already present
                echo !folderName! | findstr /i "_9999999" >nul
                if !errorlevel! == 0 set "has9999999=1"

                :: Check if _P is already present
                echo !folderName! | findstr /i "_P" >nul
                if !errorlevel! == 0 set "hasP=1"

                :: Construct new name
                set "newName=!folderName!"
                if !has9999999! == 0 set "newName=!newName!_9999999"
                if !hasP! == 0 set "newName=!newName!_P"

                :: Ensure order (_9999999 before _P)
                set "newName=!newName:_P_9999999=_9999999_P!"

                set "newName=!newName!.pak"
                echo Renaming .pak to !newName!
                ren "%%~dpnF.pak" "!newName!"

                @popd

                if !keepDir! == 0 ( @RD /S /Q "%%~dpnF" )
            ) else ( echo File is not a .pak file or a folder. )
        )
    )
)

pause
exit /b

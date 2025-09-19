@echo off
@setlocal enabledelayedexpansion

:: dont allow anything other than .pak files to be processed
set fileExt=none

:: check if anything was dropped into this script
if "%~1"=="" (
	echo No .pak file was provided. Don't forget: drag and drop the .pak file into this script!
)

:: if more than one thing was provided, deal with them properly
for %%F in (%*) do (
	if exist "%%F" (
		if exist "%%F\*" (
			echo Can't unpack a folder, please drag and drop a .pak file instead.
		) else (
			set "fileExt=%%~xF"
			:: convert to lowercase
			for %%A in (!fileExt!) do set "fileExt=%%A"
			if /i "!fileExt!"==".pak" (
				@pushd %%~dpF
				.\repak.exe --aes-key 0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74 unpack %%F --verbose
				@del ""%%F""
				@popd
			) else ( echo File provided isn't a .pak file. )
		)
	)
)

pause
exit /b
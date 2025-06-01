# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_criu_coordinator_global_optspecs
	string join \n h/help V/version
end

function __fish_criu_coordinator_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_criu_coordinator_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_criu_coordinator_using_subcommand
	set -l cmd (__fish_criu_coordinator_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -s h -l help -d 'Print help'
complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -s V -l version -d 'Print version'
complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -f -a "client" -d 'Run as client'
complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -f -a "server" -d 'Run as server'
complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -f -a "completions" -d 'Generate shell completions'
complete -c criu-coordinator -n "__fish_criu_coordinator_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -l address -d 'Address to connect the client to' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -l port -d 'Port to connect the client to' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s i -l id -d 'Unique client ID' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s d -l deps -d 'A colon-separated list of dependency IDs' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s a -l action -d 'Action name indicating the stage of checkpoint/restore' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s D -l images-dir -d 'Images directory where the stream socket is created' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s o -l log-file -d 'Log file name' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s s -l stream -d 'Use checkpoint streaming'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand client" -s h -l help -d 'Print help'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand server" -s a -l address -d 'Address to bind the server to' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand server" -s p -l port -d 'Port to bind the server to' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand server" -s o -l log-file -d 'Log file name' -r
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand server" -s h -l help -d 'Print help'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand completions" -s h -l help -d 'Print help'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand help; and not __fish_seen_subcommand_from client server completions help" -f -a "client" -d 'Run as client'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand help; and not __fish_seen_subcommand_from client server completions help" -f -a "server" -d 'Run as server'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand help; and not __fish_seen_subcommand_from client server completions help" -f -a "completions" -d 'Generate shell completions'
complete -c criu-coordinator -n "__fish_criu_coordinator_using_subcommand help; and not __fish_seen_subcommand_from client server completions help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'

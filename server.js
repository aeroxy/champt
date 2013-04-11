var http = require('http');
var fs = require('fs');
var path = require('path');
var url = require('url');
var io = require('socket.io');

var httpServer = http.createServer(function(req,res) {
	var cacheLoad;

	if(req.url == "/" || req.url == "") {
		cacheLoad = "./www/index.html";
	}
	else {
		cacheLoad = "./www" + url.parse(req.url).pathname;
	}

	console.log("Caching: " + cacheLoad);

	var httpStatusCode = 200;

	fs.exists(cacheLoad,function(statusCheck) {

		if(!statusCheck) {
			httpStatusCode = 404;
			cacheLoad = "./404.html";
		}

		var cache = fs.readFileSync(cacheLoad);
		res.end(cache);
	});
})

httpServer.listen(8000);

var webSocket = io.listen(httpServer);

webSocket.sockets.on('connection',function(socket){

	socket.on('addToMap',function(latitude,longitude) {
		console.log('user location stored:'+latitude+','+longitude);
		socket.emit('meOnMap',latitude,longitude);
		socket.broadcast.emit('reDoMap',latitude,longitude);
	});

	socket.on('update',function(name,status){
		socket.emit('meOnMap',name,status);
		socket.broadcast.emit('reDoMap',name,status);
	});

});